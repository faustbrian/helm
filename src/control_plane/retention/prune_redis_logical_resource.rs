use super::RedisLogicalPruneOptions;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError,
    run_attached_command_capture,
};
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use std::collections::BTreeMap;

const DELETE_PREFIX_SCRIPT: &str = "local prefix = ARGV[1]\n\
local cursor = '0'\n\
local keys = {}\n\
repeat\n\
    local page = redis.call('SCAN', cursor, 'MATCH', prefix .. '*', 'COUNT', 1000)\n\
    cursor = page[1]\n\
    for _, key in ipairs(page[2]) do\n\
        if string.sub(key, 1, string.len(prefix)) ~= prefix then\n\
            return redis.error_reply('cross-prefix key returned by SCAN')\n\
        end\n\
        table.insert(keys, key)\n\
    end\n\
until cursor == '0'\n\
local deleted = 0\n\
for first = 1, #keys, 1000 do\n\
    deleted = deleted + redis.call('UNLINK', unpack(keys, first, math.min(first + 999, #keys)))\n\
end\n\
return deleted";

/// Revokes one tenant identity before atomically deleting its exact key prefix.
pub(crate) async fn prune_redis_logical_resource(
    executor: &impl CommandExecutor,
    options: RedisLogicalPruneOptions<'_>,
) -> Result<(), EngineError> {
    let prefix = validate(&options)?;
    let environment = BTreeMap::from([(
        options.flavor.client_auth_environment_key().to_owned(),
        options.administrator.secret().to_owned(),
    )]);
    let revoke = command(
        &options,
        vec![
            options.flavor.client_executable().to_owned(),
            "--raw".to_owned(),
            "--user".to_owned(),
            options.administrator.username().to_owned(),
            "ACL".to_owned(),
            "DELUSER".to_owned(),
            options.credential.username().to_owned(),
        ],
        &environment,
        "revoke Redis-compatible tenant user",
    )?;
    let revoked = run_attached_command_capture(executor, options.container, &revoke).await?;
    let revoked = integer_reply(&revoked, "Redis-compatible ACL deletion")?;
    if revoked > 1 {
        return Err(EngineError::Backend {
            detail: "Redis-compatible ACL deletion returned an impossible count".to_owned(),
        });
    }

    let delete = command(
        &options,
        vec![
            options.flavor.client_executable().to_owned(),
            "--raw".to_owned(),
            "--user".to_owned(),
            options.administrator.username().to_owned(),
            "EVAL".to_owned(),
            DELETE_PREFIX_SCRIPT.to_owned(),
            "0".to_owned(),
            prefix,
        ],
        &environment,
        "delete Redis-compatible tenant prefix",
    )?;
    let deleted = run_attached_command_capture(executor, options.container, &delete).await?;
    integer_reply(&deleted, "Redis-compatible prefix deletion")?;

    Ok(())
}

fn validate(options: &RedisLogicalPruneOptions<'_>) -> Result<String, EngineError> {
    let logical = options.logical_resource;
    let credential = options.credential;
    let administrator = options.administrator;
    let implementation = options.flavor.implementation();
    let expected_kind = format!("{implementation}_acl_prefix");
    let expected_username = format!(
        "st_{}_{}",
        logical.project_id().replace('-', "_"),
        logical.service_id().replace('-', "_")
    );
    let expected_prefix = format!(
        "stackctl:{}:{}:",
        logical.project_id(),
        logical.service_id()
    );
    let fingerprint = logical
        .compatibility_fingerprint()
        .strip_prefix("sha256:")
        .unwrap_or_default();
    let administrator_id = format!("shared/{fingerprint}/{implementation}-bootstrap");
    let invalid = options.installation_id.is_empty()
        || options.timeout.is_zero()
        || fingerprint.len() != 64
        || options.container.metadata().installation_id() != options.installation_id
        || options.container.metadata().compatibility_fingerprint()
            != logical.compatibility_fingerprint()
        || logical.kind() != expected_kind
        || logical.lifecycle() == ResourceLifecycle::Active
        || logical.orphaned_at_unix_seconds().is_none()
        || logical.logical_resource_id() != credential.credential_id()
        || credential.project_id() != Some(logical.project_id())
        || credential.service_id() != logical.service_id()
        || credential.username() != expected_username
        || credential.lifecycle() != CredentialLifecycle::Disabled
        || administrator.credential_id() != administrator_id
        || administrator.project_id().is_some()
        || administrator.service_id() != implementation
        || administrator.username() != "stackctl_admin"
        || administrator.secret().is_empty()
        || administrator.lifecycle() != CredentialLifecycle::Active;
    if invalid {
        return Err(EngineError::InvalidRequest {
            detail: "Redis-compatible logical prune inputs are not exact, orphaned, and owned"
                .to_owned(),
        });
    }

    Ok(expected_prefix)
}

fn command(
    options: &RedisLogicalPruneOptions<'_>,
    arguments: Vec<String>,
    environment: &BTreeMap<String, String>,
    action: &str,
) -> Result<AttachedCommandOptions, EngineError> {
    let request = CommandRequest::new(arguments, environment.clone(), None)?;
    AttachedCommandOptions::new(request, Vec::new(), action, options.timeout)
}

fn integer_reply(output: &[u8], action: &str) -> Result<u64, EngineError> {
    let output = std::str::from_utf8(output).map_err(|error| EngineError::Backend {
        detail: format!("{action} returned invalid UTF-8: {error}"),
    })?;
    output
        .trim()
        .parse::<u64>()
        .map_err(|error| EngineError::Backend {
            detail: format!("{action} returned a malformed integer: {error}"),
        })
}
