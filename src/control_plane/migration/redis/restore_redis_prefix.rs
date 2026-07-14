use super::RedisRestoreOptions;
use crate::control_plane::engine::{
    CommandExecutor, CommandRequest, OwnedContainer, StreamingCommandOptions, run_streaming_command,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::retention::{
    BackupResourceIdentity, open_stored_backup_artifact, verify_stored_backup_artifact,
};
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const STAGE_SCRIPT: &str = "local function unhex(value)\n\
    return (string.gsub(value, '..', function(byte)\n\
        return string.char(tonumber(byte, 16))\n\
    end))\n\
end\n\
local prefix = ARGV[1]\n\
local temporary_prefix = ARGV[2]\n\
local payload = cjson.decode(ARGV[3])\n\
local cursor = '0'\n\
local stale = {}\n\
repeat\n\
    local page = redis.call('SCAN', cursor, 'MATCH', temporary_prefix .. '*', 'COUNT', 1000)\n\
    cursor = page[1]\n\
    for _, key in ipairs(page[2]) do\n\
        if string.sub(key, 1, string.len(temporary_prefix)) ~= temporary_prefix then\n\
            return redis.error_reply('cross-prefix staging key returned by SCAN')\n\
        end\n\
        table.insert(stale, key)\n\
    end\n\
until cursor == '0'\n\
for first = 1, #stale, 1000 do\n\
    redis.call('UNLINK', unpack(stale, first, math.min(first + 999, #stale)))\n\
end\n\
for _, record in ipairs(payload.records) do\n\
    local key = unhex(record.key_hex)\n\
    if string.sub(key, 1, string.len(prefix)) ~= prefix then\n\
        return redis.error_reply('restore record is outside target prefix')\n\
    end\n\
    if string.sub(record.temporary_key, 1, string.len(temporary_prefix)) ~= temporary_prefix then\n\
        return redis.error_reply('restore record has invalid staging key')\n\
    end\n\
    redis.call('RESTORE', record.temporary_key, 0, unhex(record.dump_hex), 'REPLACE')\n\
end\n\
return #payload.records";

const COMMIT_SCRIPT: &str = "local function unhex(value)\n\
    return (string.gsub(value, '..', function(byte)\n\
        return string.char(tonumber(byte, 16))\n\
    end))\n\
end\n\
local prefix = ARGV[1]\n\
local temporary_prefix = ARGV[2]\n\
local payload = cjson.decode(ARGV[3])\n\
for _, record in ipairs(payload.records) do\n\
    local key = unhex(record.key_hex)\n\
    if string.sub(key, 1, string.len(prefix)) ~= prefix then\n\
        return redis.error_reply('restore record is outside target prefix')\n\
    end\n\
    if string.sub(record.temporary_key, 1, string.len(temporary_prefix)) ~= temporary_prefix then\n\
        return redis.error_reply('restore record has invalid staging key')\n\
    end\n\
    if redis.call('EXISTS', record.temporary_key) ~= 1 then\n\
        return redis.error_reply('restore staging key is missing')\n\
    end\n\
end\n\
local cursor = '0'\n\
local current = {}\n\
repeat\n\
    local page = redis.call('SCAN', cursor, 'MATCH', prefix .. '*', 'COUNT', 1000)\n\
    cursor = page[1]\n\
    for _, key in ipairs(page[2]) do\n\
        if string.sub(key, 1, string.len(prefix)) ~= prefix then\n\
            return redis.error_reply('cross-prefix target key returned by SCAN')\n\
        end\n\
        table.insert(current, key)\n\
    end\n\
until cursor == '0'\n\
for first = 1, #current, 1000 do\n\
    redis.call('UNLINK', unpack(current, first, math.min(first + 999, #current)))\n\
end\n\
for _, record in ipairs(payload.records) do\n\
    local key = unhex(record.key_hex)\n\
    redis.call('RENAME', record.temporary_key, key)\n\
    if record.ttl_milliseconds ~= '0' then\n\
        redis.call('PEXPIRE', key, record.ttl_milliseconds)\n\
    end\n\
end\n\
return #payload.records";

/// Stages verified serialized values before atomically replacing one prefix.
pub(crate) async fn restore_redis_prefix(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &RedisRestoreOptions<'_>,
) -> Result<(), MigrationOperationError> {
    validate(container, options)?;
    let identity =
        BackupResourceIdentity::from_logical(options.logical_resource, options.installation_id);
    let stored =
        open_stored_backup_artifact(options.recovery_point.reference()).map_err(|error| {
            operation_error("Redis-compatible restore backup is unavailable", error)
        })?;
    let evidence = verify_stored_backup_artifact(&stored, options.restored_at_unix_seconds)
        .map_err(|error| {
            operation_error("Redis-compatible restore backup verification failed", error)
        })?;
    if !evidence.matches_identity(&identity)
        || evidence.artifact_sha256() != options.recovery_point.artifact_sha256()
        || evidence.artifact_size_bytes() != options.recovery_point.artifact_size_bytes()
    {
        return Err(MigrationOperationError::new(
            "Redis-compatible restore backup does not match its recovery point",
        ));
    }
    let snapshot_bytes = tokio::fs::read(stored.artifact_file())
        .await
        .map_err(|error| operation_error("Redis-compatible restore artifact read failed", error))?;
    let snapshot_size = u64::try_from(snapshot_bytes.len()).unwrap_or(u64::MAX);
    let snapshot_sha256 = hex::encode(Sha256::digest(&snapshot_bytes));
    if snapshot_size != options.recovery_point.artifact_size_bytes()
        || snapshot_sha256 != options.recovery_point.artifact_sha256()
    {
        return Err(MigrationOperationError::new(
            "Redis-compatible restore artifact changed after verification",
        ));
    }
    let payload = restore_payload(&snapshot_bytes, options)?;
    let payload = serde_json::to_vec(&payload)
        .map_err(|error| operation_error("Redis-compatible restore payload is invalid", error))?;
    let temporary_prefix = temporary_prefix(options);

    run_script(
        executor,
        container,
        options,
        STAGE_SCRIPT,
        &temporary_prefix,
        &payload,
        "stage Redis-compatible restore records",
    )
    .await?;
    run_script(
        executor,
        container,
        options,
        COMMIT_SCRIPT,
        &temporary_prefix,
        &payload,
        "commit Redis-compatible prefix restore",
    )
    .await
}

async fn run_script(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &RedisRestoreOptions<'_>,
    script: &str,
    temporary_prefix: &str,
    payload: &[u8],
    action: &str,
) -> Result<(), MigrationOperationError> {
    let request = CommandRequest::new(
        vec![
            options.flavor.client_executable().to_owned(),
            "--raw".to_owned(),
            "-e".to_owned(),
            "-x".to_owned(),
            "--user".to_owned(),
            options.administrator.username().to_owned(),
            "EVAL".to_owned(),
            script.to_owned(),
            "0".to_owned(),
            options.prefix.to_owned(),
            temporary_prefix.to_owned(),
        ],
        BTreeMap::from([(
            options.flavor.client_auth_environment_key().to_owned(),
            options.administrator.secret().to_owned(),
        )]),
        None,
    )
    .map_err(|error| operation_error("Redis-compatible restore request is invalid", error))?;
    let command = StreamingCommandOptions::new(request, action, options.timeout)
        .map_err(|error| operation_error("Redis-compatible restore request is invalid", error))?;
    let mut input = payload;
    let mut output = tokio::io::sink();
    run_streaming_command(executor, container, &command, &mut input, &mut output)
        .await
        .map_err(|error| operation_error("Redis-compatible restore command failed", error))
}

fn validate(
    container: &OwnedContainer,
    options: &RedisRestoreOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let logical = options.logical_resource;
    let credential = options.credential;
    let administrator = options.administrator;
    let recovery = options.recovery_point;
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
    let exact_recovery = recovery.project_id() == logical.project_id()
        && recovery.service_id() == logical.service_id()
        && recovery.logical_resource_id() == logical.logical_resource_id()
        && recovery.resource_kind() == logical.kind()
        && recovery.compatibility_fingerprint() == logical.compatibility_fingerprint();
    let invalid = options.installation_id.is_empty()
        || options.timeout.is_zero()
        || fingerprint.len() != 64
        || options.restored_at_unix_seconds < recovery.verified_at_unix_seconds()
        || options.prefix != expected_prefix
        || container.metadata().installation_id() != options.installation_id
        || container.metadata().compatibility_fingerprint() != logical.compatibility_fingerprint()
        || logical.kind() != expected_kind
        || logical.lifecycle() != ResourceLifecycle::Active
        || logical.logical_resource_id() != credential.credential_id()
        || credential.project_id() != Some(logical.project_id())
        || credential.service_id() != logical.service_id()
        || credential.username() != expected_username
        || credential.lifecycle() != CredentialLifecycle::Active
        || administrator.credential_id() != administrator_id
        || administrator.project_id().is_some()
        || administrator.service_id() != implementation
        || administrator.username() != "stackctl_admin"
        || administrator.secret().is_empty()
        || administrator.lifecycle() != CredentialLifecycle::Active
        || !exact_recovery;
    if invalid {
        return Err(MigrationOperationError::new(
            "Redis-compatible restore request does not match an active owned prefix",
        ));
    }

    Ok(())
}

fn restore_payload(
    bytes: &[u8],
    options: &RedisRestoreOptions<'_>,
) -> Result<RestorePayload, MigrationOperationError> {
    let snapshot = serde_json::from_slice::<PrefixSnapshot>(bytes).map_err(|error| {
        operation_error("Redis-compatible restore snapshot is malformed", error)
    })?;
    let prefix = decode_hex(&snapshot.prefix_hex, "snapshot prefix")?;
    if snapshot.format != 1
        || snapshot.created_at_unix_seconds != options.recovery_point.created_at_unix_seconds()
        || prefix != options.prefix.as_bytes()
    {
        return Err(MigrationOperationError::new(
            "Redis-compatible restore snapshot identity does not match its recovery point",
        ));
    }
    let elapsed_seconds = options
        .restored_at_unix_seconds
        .checked_sub(snapshot.created_at_unix_seconds)
        .ok_or_else(|| MigrationOperationError::new("Redis-compatible restore time is invalid"))?;
    let elapsed_milliseconds = elapsed_seconds.checked_mul(1_000).ok_or_else(|| {
        MigrationOperationError::new("Redis-compatible restore TTL elapsed time overflowed")
    })?;
    let mut keys = BTreeSet::new();
    let mut records = Vec::new();
    for record in snapshot.records.into_records()? {
        let key = decode_hex(&record.key_hex, "snapshot key")?;
        let dump = decode_hex(&record.dump_hex, "snapshot value")?;
        if !key.starts_with(options.prefix.as_bytes())
            || dump.is_empty()
            || record.ttl_milliseconds < -1
            || !keys.insert(key)
        {
            return Err(MigrationOperationError::new(
                "Redis-compatible restore snapshot contains invalid or duplicate records",
            ));
        }
        let ttl = match record.ttl_milliseconds {
            -1 => 0,
            ttl if ttl > elapsed_milliseconds => ttl - elapsed_milliseconds,
            _ => continue,
        };
        records.push(RestoreRecord {
            key_hex: record.key_hex,
            dump_hex: record.dump_hex,
            ttl_milliseconds: ttl.to_string(),
            temporary_key: String::new(),
        });
    }
    records.sort_by(|left, right| left.key_hex.cmp(&right.key_hex));
    let temporary_prefix = temporary_prefix(options);
    for (index, record) in records.iter_mut().enumerate() {
        record.temporary_key = format!("{temporary_prefix}{index}");
    }

    Ok(RestorePayload { records })
}

fn temporary_prefix(options: &RedisRestoreOptions<'_>) -> String {
    let mut digest = Sha256::new();
    for value in [
        "stackctl-redis-restore-v1",
        options.recovery_point.recovery_point_id(),
        options.logical_resource.logical_resource_id(),
        options.recovery_point.artifact_sha256(),
    ] {
        digest.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
        digest.update(value.as_bytes());
    }
    format!("__stackctl_restore__:{}:", hex::encode(digest.finalize()))
}

fn decode_hex(value: &str, field: &str) -> Result<Vec<u8>, MigrationOperationError> {
    hex::decode(value)
        .map_err(|error| operation_error(&format!("Redis-compatible {field} is invalid"), error))
}

#[derive(Serialize)]
struct RestorePayload {
    records: Vec<RestoreRecord>,
}

#[derive(Serialize)]
struct RestoreRecord {
    key_hex: String,
    dump_hex: String,
    ttl_milliseconds: String,
    temporary_key: String,
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
                "Redis-compatible restore snapshot records are malformed",
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
