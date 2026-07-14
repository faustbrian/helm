use super::MinioLogicalPruneOptions;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, run_attached_command,
    run_attached_command_capture,
};
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use serde::Deserialize;
use std::collections::BTreeMap;

const ALIAS: &str = "stackctl";

/// Idempotently removes one exact bucket, identity, and policy.
pub(crate) async fn prune_minio_logical_resource(
    executor: &impl CommandExecutor,
    options: MinioLogicalPruneOptions<'_>,
) -> Result<(), EngineError> {
    let identity = validate(&options)?;
    let environment = environment(options.administrator);
    if present(
        executor,
        &options,
        vec!["mc", "admin", "user", "list", ALIAS, "--json"],
        &environment,
        InventoryIdentity::User(&identity.username),
        "list MinIO users before logical prune",
    )
    .await?
    {
        run(
            executor,
            &options,
            vec!["mc", "admin", "user", "remove", ALIAS, &identity.username],
            &environment,
            "remove confirmed MinIO tenant user",
        )
        .await?;
    }
    if present(
        executor,
        &options,
        vec!["mc", "ls", ALIAS, "--json"],
        &environment,
        InventoryIdentity::Bucket(&identity.bucket),
        "list MinIO buckets before logical prune",
    )
    .await?
    {
        let bucket = format!("{ALIAS}/{}", identity.bucket);
        run(
            executor,
            &options,
            vec!["mc", "rb", "--force", &bucket],
            &environment,
            "remove confirmed MinIO tenant bucket",
        )
        .await?;
    }
    if present(
        executor,
        &options,
        vec!["mc", "admin", "policy", "list", ALIAS, "--json"],
        &environment,
        InventoryIdentity::Policy(&identity.bucket),
        "list MinIO policies before logical prune",
    )
    .await?
    {
        run(
            executor,
            &options,
            vec!["mc", "admin", "policy", "remove", ALIAS, &identity.bucket],
            &environment,
            "remove confirmed MinIO tenant policy",
        )
        .await?;
    }

    Ok(())
}

struct TenantIdentity {
    bucket: String,
    username: String,
}

fn validate(options: &MinioLogicalPruneOptions<'_>) -> Result<TenantIdentity, EngineError> {
    let logical = options.logical_resource;
    let credential = options.credential;
    let administrator = options.administrator;
    let identity = format!("{}-{}", logical.project_id(), logical.service_id());
    let bucket = format!("stackctl-{identity}");
    let username = format!("st_{}", identity.replace('-', "_"));
    let fingerprint = logical
        .compatibility_fingerprint()
        .strip_prefix("sha256:")
        .unwrap_or_default();
    let expected_administrator_id = format!("shared/{fingerprint}/minio-root");
    let invalid = options.installation_id.is_empty()
        || options.timeout.is_zero()
        || fingerprint.len() != 64
        || options.container.metadata().installation_id() != options.installation_id
        || options.container.metadata().compatibility_fingerprint()
            != logical.compatibility_fingerprint()
        || logical.kind() != "minio_bucket_policy"
        || logical.lifecycle() == ResourceLifecycle::Active
        || logical.orphaned_at_unix_seconds().is_none()
        || logical.logical_resource_id() != credential.credential_id()
        || credential.project_id() != Some(logical.project_id())
        || credential.service_id() != logical.service_id()
        || credential.username() != username
        || credential.lifecycle() != CredentialLifecycle::Disabled
        || bucket.len() > 63
        || administrator.credential_id() != expected_administrator_id
        || administrator.project_id().is_some()
        || administrator.service_id() != "minio"
        || administrator.username() != "stackctl_admin"
        || administrator.secret().is_empty()
        || administrator.lifecycle() != CredentialLifecycle::Active;
    if invalid {
        return Err(EngineError::InvalidRequest {
            detail: "MinIO logical prune inputs are not exact, orphaned, and owned".to_owned(),
        });
    }

    Ok(TenantIdentity { bucket, username })
}

enum InventoryIdentity<'identity> {
    User(&'identity str),
    Bucket(&'identity str),
    Policy(&'identity str),
}

async fn present(
    executor: &impl CommandExecutor,
    options: &MinioLogicalPruneOptions<'_>,
    arguments: Vec<&str>,
    environment: &BTreeMap<String, String>,
    expected: InventoryIdentity<'_>,
    action: &str,
) -> Result<bool, EngineError> {
    let command = command(arguments, environment, action, options.timeout)?;
    let output = run_attached_command_capture(executor, options.container, &command).await?;
    let rows = serde_json::Deserializer::from_slice(&output).into_iter::<InventoryRow>();
    let mut found = false;
    for row in rows {
        let row = row.map_err(|error| EngineError::Backend {
            detail: format!("{action} returned malformed JSON: {error}"),
        })?;
        if row.status != "success" {
            return Err(EngineError::Backend {
                detail: format!("{action} returned non-success status '{}'", row.status),
            });
        }
        found |= match expected {
            InventoryIdentity::User(value) => required(row.access_key, action)? == value,
            InventoryIdentity::Policy(value) => required(row.policy, action)? == value,
            InventoryIdentity::Bucket(value) => {
                let kind = required(row.kind, action)?;
                let key = required(row.key, action)?;
                kind == "folder" && key.strip_suffix('/').unwrap_or(&key) == value
            }
        };
    }

    Ok(found)
}

async fn run(
    executor: &impl CommandExecutor,
    options: &MinioLogicalPruneOptions<'_>,
    arguments: Vec<&str>,
    environment: &BTreeMap<String, String>,
    action: &str,
) -> Result<(), EngineError> {
    let command = command(arguments, environment, action, options.timeout)?;
    run_attached_command(executor, options.container, &command).await
}

fn command(
    arguments: Vec<&str>,
    environment: &BTreeMap<String, String>,
    action: &str,
    timeout: std::time::Duration,
) -> Result<AttachedCommandOptions, EngineError> {
    let request = CommandRequest::new(
        arguments.into_iter().map(str::to_owned).collect(),
        environment.clone(),
        None,
    )?;
    AttachedCommandOptions::new(request, Vec::new(), action, timeout)
}

fn environment(
    administrator: &crate::control_plane::state::CredentialRecord,
) -> BTreeMap<String, String> {
    BTreeMap::from([(
        "MC_HOST_stackctl".to_owned(),
        format!(
            "http://{}:{}@127.0.0.1:9000",
            administrator.username(),
            administrator.secret()
        ),
    )])
}

fn required(value: Option<String>, action: &str) -> Result<String, EngineError> {
    value.ok_or_else(|| EngineError::Backend {
        detail: format!("{action} returned an incomplete inventory row"),
    })
}

#[derive(Deserialize)]
struct InventoryRow {
    status: String,
    #[serde(rename = "type")]
    kind: Option<String>,
    key: Option<String>,
    #[serde(rename = "accessKey")]
    access_key: Option<String>,
    policy: Option<String>,
}
