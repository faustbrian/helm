use super::{V7RedisCredential, validate_redis_prefix_snapshot};
use crate::control_plane::engine::{
    CommandRequest, StreamingCommandOptions, V7ContainerCommandExecutor, V7ContainerCommandTarget,
    run_v7_streaming_command,
};
use crate::control_plane::migration::{MigrationBackup, MigrationOperationError};
use crate::control_plane::retention::{
    BackupResourceIdentity, store_backup_artifact_from_async_reader, verify_stored_backup_artifact,
};
use crate::control_plane::shared_infrastructure::RedisFlavor;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncWriteExt, duplex};

const STREAM_BUFFER_BYTES: usize = 64 * 1024;
const SNAPSHOT_SCRIPT: &str = "local function hex(value) return (string.gsub(value, '.', function(byte) return string.format('%02x', string.byte(byte)) end)) end\nlocal target_prefix = ARGV[1]\nlocal cursor = '0'\nlocal records = {}\nrepeat\n local page = redis.call('SCAN', cursor, 'COUNT', 1000)\n cursor = page[1]\n for _, source_key in ipairs(page[2]) do\n  local dump = redis.call('DUMP', source_key)\n  if dump then table.insert(records, { key_hex = hex(target_prefix .. source_key), dump_hex = hex(dump), ttl_milliseconds = redis.call('PTTL', source_key) }) end\n end\nuntil cursor == '0'\ntable.sort(records, function(left, right) return left.key_hex < right.key_hex end)\nreturn cjson.encode({ format = 1, created_at_unix_seconds = tonumber(ARGV[2]), prefix_hex = hex(target_prefix), records = records })";

/// Exports an accepted v7 logical database while namespacing every target key.
pub(super) async fn backup_v7_redis_keyspace(
    executor: &(impl V7ContainerCommandExecutor + Sync),
    target: &V7ContainerCommandTarget,
    flavor: RedisFlavor,
    credential: &V7RedisCredential,
    target_prefix: &str,
    identity: &BackupResourceIdentity,
    backup_root: &Path,
    created_at_unix_seconds: i64,
    verified_at_unix_seconds: i64,
    timeout: Duration,
) -> Result<MigrationBackup, MigrationOperationError> {
    let mut environment = BTreeMap::new();
    if !credential.password().is_empty() {
        environment.insert(
            flavor.client_auth_environment_key().to_owned(),
            credential.password().to_owned(),
        );
    }
    let request = CommandRequest::new(
        vec![
            flavor.client_executable().to_owned(),
            "--raw".to_owned(),
            "--user".to_owned(),
            credential.username().to_owned(),
            "-n".to_owned(),
            credential.database().to_string(),
            "EVAL".to_owned(),
            SNAPSHOT_SCRIPT.to_owned(),
            "0".to_owned(),
            target_prefix.to_owned(),
            created_at_unix_seconds.to_string(),
        ],
        environment,
        None,
    )
    .map_err(|error| operation_error("v7 Redis-compatible backup request is invalid", error))?;
    let command = StreamingCommandOptions::new(
        request,
        "export accepted v7 Redis-compatible keyspace",
        timeout,
    )
    .map_err(|error| operation_error("v7 Redis-compatible backup request is invalid", error))?;
    let (mut reader, mut output) = duplex(STREAM_BUFFER_BYTES);
    let mut input = tokio::io::empty();
    let export = async {
        let result =
            run_v7_streaming_command(executor, target, &command, &mut input, &mut output).await;
        let close = output.shutdown().await;
        result.map_err(|error| operation_error("v7 Redis-compatible backup failed", error))?;
        close.map_err(|error| operation_error("v7 Redis-compatible backup close failed", error))
    };
    let store = async {
        store_backup_artifact_from_async_reader(
            identity,
            &mut reader,
            created_at_unix_seconds,
            backup_root,
        )
        .await
        .map_err(|error| operation_error("v7 Redis-compatible backup storage failed", error))
    };
    let (_, stored) = futures_util::future::try_join(export, store).await?;
    validate_redis_prefix_snapshot(&stored, target_prefix, created_at_unix_seconds).await?;
    let evidence =
        verify_stored_backup_artifact(&stored, verified_at_unix_seconds).map_err(|error| {
            operation_error("v7 Redis-compatible backup verification failed", error)
        })?;
    let reference = stored.recovery_point().to_str().ok_or_else(|| {
        MigrationOperationError::new("v7 Redis-compatible recovery path is not valid Unicode")
    })?;
    MigrationBackup::new(
        reference,
        evidence.artifact_sha256(),
        evidence.artifact_size_bytes(),
    )
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
