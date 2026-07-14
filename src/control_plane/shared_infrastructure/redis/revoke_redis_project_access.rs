use super::RedisAccessRevocationOptions;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError,
    run_attached_command_capture,
};
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use std::collections::BTreeMap;

/// Disables one exact tenant ACL user without deleting its namespaced keys.
pub(crate) async fn revoke_redis_project_access(
    executor: &impl CommandExecutor,
    options: RedisAccessRevocationOptions<'_>,
) -> Result<bool, EngineError> {
    validate(&options)?;
    let environment = BTreeMap::from([(
        options.flavor.client_auth_environment_key().to_owned(),
        options.administrator.secret().to_owned(),
    )]);
    let request = CommandRequest::new(
        vec![
            options.flavor.client_executable().to_owned(),
            "--raw".to_owned(),
            "--user".to_owned(),
            options.administrator.username().to_owned(),
            "ACL".to_owned(),
            "DELUSER".to_owned(),
            options.credential.username().to_owned(),
        ],
        environment,
        None,
    )?;
    let command = AttachedCommandOptions::new(
        request,
        Vec::new(),
        format!(
            "revoke orphaned {} tenant access",
            options.flavor.implementation()
        ),
        options.timeout,
    )?;
    let output = run_attached_command_capture(executor, options.container, &command).await?;
    let revoked = integer_reply(&output)?;
    if revoked > 1 {
        return Err(EngineError::Backend {
            detail: "Redis-compatible ACL deletion returned an impossible count".to_owned(),
        });
    }

    Ok(revoked == 1)
}

fn validate(options: &RedisAccessRevocationOptions<'_>) -> Result<(), EngineError> {
    let logical = options.logical_resource;
    let credential = options.credential;
    let administrator = options.administrator;
    let implementation = options.flavor.implementation();
    let fingerprint = logical
        .compatibility_fingerprint()
        .strip_prefix("sha256:")
        .unwrap_or_default();
    let expected_administrator = format!("shared/{fingerprint}/{implementation}-bootstrap");
    let expected_username = format!(
        "st_{}_{}",
        logical.project_id().replace('-', "_"),
        logical.service_id().replace('-', "_")
    );
    let invalid = options.installation_id.is_empty()
        || options.timeout.is_zero()
        || fingerprint.len() != 64
        || options.container.metadata().installation_id() != options.installation_id
        || options.container.metadata().compatibility_fingerprint()
            != logical.compatibility_fingerprint()
        || logical.kind() != format!("{implementation}_acl_prefix")
        || logical.lifecycle() != ResourceLifecycle::Orphaned
        || logical.orphaned_at_unix_seconds().is_none()
        || logical.logical_resource_id() != credential.credential_id()
        || credential.project_id() != Some(logical.project_id())
        || credential.service_id() != logical.service_id()
        || credential.username() != expected_username
        || credential.lifecycle() != CredentialLifecycle::Disabled
        || administrator.credential_id() != expected_administrator
        || administrator.project_id().is_some()
        || administrator.service_id() != implementation
        || administrator.username() != "stackctl_admin"
        || administrator.secret().is_empty()
        || administrator.lifecycle() != CredentialLifecycle::Active;
    if invalid {
        return Err(EngineError::InvalidRequest {
            detail: "Redis-compatible access revocation inputs are not exact, orphaned, and owned"
                .to_owned(),
        });
    }

    Ok(())
}

fn integer_reply(output: &[u8]) -> Result<u64, EngineError> {
    let output = std::str::from_utf8(output).map_err(|error| EngineError::Backend {
        detail: format!("Redis-compatible ACL deletion returned invalid UTF-8: {error}"),
    })?;
    output
        .trim()
        .parse::<u64>()
        .map_err(|error| EngineError::Backend {
            detail: format!("Redis-compatible ACL deletion returned a malformed integer: {error}"),
        })
}
