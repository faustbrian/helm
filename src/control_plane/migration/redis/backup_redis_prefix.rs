use super::RedisBackupOptions;
use crate::control_plane::engine::{
    CommandExecutor, CommandRequest, OwnedContainer, StreamingCommandOptions, run_streaming_command,
};
use crate::control_plane::migration::{MigrationBackup, MigrationOperationError};
use crate::control_plane::retention::{
    BackupResourceIdentity, store_backup_artifact_from_async_reader, verify_stored_backup_artifact,
};
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use tokio::io::{AsyncWriteExt, duplex};

const STREAM_BUFFER_BYTES: usize = 64 * 1024;
const SNAPSHOT_SCRIPT: &str = "local function hex(value)\n\
    return (string.gsub(value, '.', function(byte)\n\
        return string.format('%02x', string.byte(byte))\n\
    end))\n\
end\n\
local prefix = ARGV[1]\n\
local cursor = '0'\n\
local records = {}\n\
repeat\n\
    local page = redis.call('SCAN', cursor, 'MATCH', prefix .. '*', 'COUNT', 1000)\n\
    cursor = page[1]\n\
    for _, key in ipairs(page[2]) do\n\
        local dump = redis.call('DUMP', key)\n\
        if dump then\n\
            table.insert(records, {\n\
                key_hex = hex(key),\n\
                dump_hex = hex(dump),\n\
                ttl_milliseconds = redis.call('PTTL', key)\n\
            })\n\
        end\n\
    end\n\
until cursor == '0'\n\
table.sort(records, function(left, right) return left.key_hex < right.key_hex end)\n\
return cjson.encode({\n\
    format = 1,\n\
    created_at_unix_seconds = tonumber(ARGV[2]),\n\
    prefix_hex = hex(prefix),\n\
    records = records\n\
})";

/// Streams one atomic, binary-safe namespace snapshot into immutable storage.
pub(crate) async fn backup_redis_prefix(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &RedisBackupOptions<'_>,
) -> Result<MigrationBackup, MigrationOperationError> {
    validate(container, options)?;
    let request = CommandRequest::new(
        vec![
            options.flavor.client_executable().to_owned(),
            "--raw".to_owned(),
            "--user".to_owned(),
            options.administrator.username().to_owned(),
            "EVAL".to_owned(),
            SNAPSHOT_SCRIPT.to_owned(),
            "0".to_owned(),
            options.prefix.to_owned(),
            options.created_at_unix_seconds.to_string(),
        ],
        BTreeMap::from([(
            options.flavor.client_auth_environment_key().to_owned(),
            options.administrator.secret().to_owned(),
        )]),
        None,
    )
    .map_err(|error| operation_error("Redis-compatible backup request is invalid", error))?;
    let command = StreamingCommandOptions::new(
        request,
        "export Redis-compatible key prefix",
        options.timeout,
    )
    .map_err(|error| operation_error("Redis-compatible backup request is invalid", error))?;
    let identity =
        BackupResourceIdentity::from_logical(options.logical_resource, options.installation_id);
    let (mut backup_reader, mut command_output) = duplex(STREAM_BUFFER_BYTES);
    let mut command_input = tokio::io::empty();
    let export = async {
        let result = run_streaming_command(
            executor,
            container,
            &command,
            &mut command_input,
            &mut command_output,
        )
        .await;
        let close = command_output.shutdown().await;
        result.map_err(|error| operation_error("Redis-compatible backup failed", error))?;
        close.map_err(|error| operation_error("Redis-compatible backup output close failed", error))
    };
    let store = async {
        store_backup_artifact_from_async_reader(
            &identity,
            &mut backup_reader,
            options.created_at_unix_seconds,
            options.backup_root,
        )
        .await
        .map_err(|error| operation_error("Redis-compatible backup storage failed", error))
    };
    let (_, stored) = futures_util::future::try_join(export, store).await?;
    validate_snapshot(&stored, options).await?;
    let evidence = verify_stored_backup_artifact(&stored, options.created_at_unix_seconds)
        .map_err(|error| operation_error("Redis-compatible backup verification failed", error))?;
    let reference = stored.recovery_point().to_str().ok_or_else(|| {
        MigrationOperationError::new("Redis-compatible backup recovery point is not valid Unicode")
    })?;

    MigrationBackup::new(
        reference,
        evidence.artifact_sha256(),
        evidence.artifact_size_bytes(),
    )
}

fn validate(
    container: &OwnedContainer,
    options: &RedisBackupOptions<'_>,
) -> Result<(), MigrationOperationError> {
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
        || options.created_at_unix_seconds < 0
        || !options.backup_root.is_absolute()
        || options.timeout.is_zero()
        || logical.kind() != expected_kind
        || logical.lifecycle() != ResourceLifecycle::Active
        || logical.logical_resource_id() != credential.credential_id()
        || credential.project_id() != Some(logical.project_id())
        || credential.service_id() != logical.service_id()
        || credential.username() != expected_username
        || credential.lifecycle() != CredentialLifecycle::Active
        || options.prefix != expected_prefix
        || administrator.credential_id() != administrator_id
        || administrator.project_id().is_some()
        || administrator.service_id() != implementation
        || administrator.username() != "stackctl_admin"
        || administrator.secret().is_empty()
        || administrator.lifecycle() != CredentialLifecycle::Active
        || container.metadata().installation_id() != options.installation_id
        || container.metadata().compatibility_fingerprint() != logical.compatibility_fingerprint();
    if invalid {
        return Err(MigrationOperationError::new(
            "Redis-compatible backup request does not match an active owned prefix",
        ));
    }

    Ok(())
}

async fn validate_snapshot(
    stored: &crate::control_plane::retention::StoredBackupArtifact,
    options: &RedisBackupOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let bytes = tokio::fs::read(stored.artifact_file())
        .await
        .map_err(|error| operation_error("Redis-compatible snapshot could not be read", error))?;
    let snapshot = serde_json::from_slice::<PrefixSnapshot>(&bytes)
        .map_err(|error| operation_error("Redis-compatible snapshot is malformed", error))?;
    let prefix = decode_hex(&snapshot.prefix_hex, "snapshot prefix")?;
    if snapshot.format != 1
        || snapshot.created_at_unix_seconds != options.created_at_unix_seconds
        || prefix != options.prefix.as_bytes()
    {
        return Err(MigrationOperationError::new(
            "Redis-compatible snapshot identity does not match its backup request",
        ));
    }
    let mut keys = BTreeSet::new();
    for record in snapshot.records.into_records()? {
        let key = decode_hex(&record.key_hex, "snapshot key")?;
        let dump = decode_hex(&record.dump_hex, "snapshot value")?;
        if !key.starts_with(options.prefix.as_bytes())
            || dump.is_empty()
            || record.ttl_milliseconds < -1
            || !keys.insert(key)
        {
            return Err(MigrationOperationError::new(
                "Redis-compatible snapshot contains invalid or duplicate prefix records",
            ));
        }
    }

    Ok(())
}

fn decode_hex(value: &str, field: &str) -> Result<Vec<u8>, MigrationOperationError> {
    hex::decode(value)
        .map_err(|error| operation_error(&format!("Redis-compatible {field} is invalid"), error))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrefixSnapshot {
    format: u32,
    created_at_unix_seconds: i64,
    prefix_hex: String,
    records: SnapshotRecords,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SnapshotRecords {
    Records(Vec<PrefixRecord>),
    Empty(BTreeMap<String, serde_json::Value>),
}

impl SnapshotRecords {
    fn into_records(self) -> Result<Vec<PrefixRecord>, MigrationOperationError> {
        match self {
            Self::Records(records) => Ok(records),
            Self::Empty(values) if values.is_empty() => Ok(Vec::new()),
            Self::Empty(_) => Err(MigrationOperationError::new(
                "Redis-compatible snapshot records are malformed",
            )),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrefixRecord {
    key_hex: String,
    dump_hex: String,
    ttl_milliseconds: i64,
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
