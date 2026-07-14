use super::{MinioAccessRevocationOptions, ObjectStoreProjectDefinition};
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, run_attached_command,
    run_attached_command_capture,
};
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use std::collections::BTreeMap;

const ALIAS: &str = "stackctl";

/// Disables one exact MinIO tenant identity while retaining buckets and objects.
pub(crate) async fn revoke_minio_project_access(
    executor: &impl CommandExecutor,
    options: MinioAccessRevocationOptions<'_>,
) -> Result<bool, EngineError> {
    let definition = validate(&options)?;
    let environment = BTreeMap::from([(
        "MC_HOST_stackctl".to_owned(),
        format!(
            "http://{}:{}@127.0.0.1:9000",
            options.administrator.username(),
            options.administrator.secret()
        ),
    )]);
    let list = CommandRequest::new(
        vec![
            "mc".to_owned(),
            "admin".to_owned(),
            "user".to_owned(),
            "list".to_owned(),
            "--json".to_owned(),
            ALIAS.to_owned(),
        ],
        environment.clone(),
        None,
    )?;
    let list = AttachedCommandOptions::new(
        list,
        Vec::new(),
        "list MinIO users before tenant access revocation".to_owned(),
        options.timeout,
    )?;
    let output = run_attached_command_capture(executor, options.container, &list).await?;
    if !enabled_user(&output, definition.username())? {
        return Ok(false);
    }

    let disable = CommandRequest::new(
        vec![
            "mc".to_owned(),
            "admin".to_owned(),
            "user".to_owned(),
            "disable".to_owned(),
            ALIAS.to_owned(),
            definition.username().to_owned(),
        ],
        environment,
        None,
    )?;
    let disable = AttachedCommandOptions::new(
        disable,
        Vec::new(),
        "disable orphaned MinIO tenant access".to_owned(),
        options.timeout,
    )?;
    run_attached_command(executor, options.container, &disable).await?;

    Ok(true)
}

fn validate(
    options: &MinioAccessRevocationOptions<'_>,
) -> Result<ObjectStoreProjectDefinition, EngineError> {
    let logical = options.logical_resource;
    let credential = options.credential;
    let administrator = options.administrator;
    let fingerprint = logical
        .compatibility_fingerprint()
        .strip_prefix("sha256:")
        .unwrap_or_default();
    let expected_administrator = format!("shared/{fingerprint}/minio-root");
    let expected_credential = format!(
        "{}/{}/object-store",
        logical.project_id(),
        logical.service_id()
    );
    let definition = ObjectStoreProjectDefinition::new(
        logical.project_id(),
        logical.service_id(),
        CredentialSecret::new(credential.secret().to_owned()),
    )
    .map_err(|error| EngineError::InvalidRequest {
        detail: error.to_string(),
    })?;
    let invalid = options.installation_id.is_empty()
        || options.timeout.is_zero()
        || fingerprint.len() != 64
        || options.container.metadata().installation_id() != options.installation_id
        || options.container.metadata().compatibility_fingerprint()
            != logical.compatibility_fingerprint()
        || logical.kind() != "minio_bucket_policy"
        || logical.lifecycle() != ResourceLifecycle::Orphaned
        || logical.orphaned_at_unix_seconds().is_none()
        || logical.logical_resource_id() != expected_credential
        || credential.credential_id() != expected_credential
        || credential.project_id() != Some(logical.project_id())
        || credential.service_id() != logical.service_id()
        || credential.username() != definition.username()
        || credential.lifecycle() != CredentialLifecycle::Disabled
        || administrator.credential_id() != expected_administrator
        || administrator.project_id().is_some()
        || administrator.service_id() != "minio"
        || administrator.username() != "stackctl_admin"
        || administrator.secret().is_empty()
        || administrator.lifecycle() != CredentialLifecycle::Active;
    if invalid {
        return Err(EngineError::InvalidRequest {
            detail: "MinIO access revocation inputs are not exact, orphaned, and owned".to_owned(),
        });
    }

    Ok(definition)
}

fn enabled_user(output: &[u8], username: &str) -> Result<bool, EngineError> {
    let output = std::str::from_utf8(output).map_err(|error| EngineError::Backend {
        detail: format!("MinIO user listing returned invalid UTF-8: {error}"),
    })?;
    let mut enabled = None;
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|error| EngineError::Backend {
                detail: format!("MinIO user listing returned malformed JSON: {error}"),
            })?;
        if value.get("accessKey").and_then(serde_json::Value::as_str) != Some(username) {
            continue;
        }
        if enabled.is_some() {
            return Err(EngineError::Backend {
                detail: "MinIO user listing returned the tenant identity more than once".to_owned(),
            });
        }
        enabled = Some(
            match value.get("userStatus").and_then(serde_json::Value::as_str) {
                Some("enabled") => true,
                Some("disabled") => false,
                _ => {
                    return Err(EngineError::Backend {
                        detail: "MinIO user listing returned an unknown tenant status".to_owned(),
                    });
                }
            },
        );
    }

    Ok(enabled.unwrap_or(false))
}
