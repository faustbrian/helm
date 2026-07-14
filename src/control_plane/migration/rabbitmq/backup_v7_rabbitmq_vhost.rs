use super::{V7RabbitMqCredential, transform_v7_rabbitmq_definitions};
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandRequest, V7ContainerCommandExecutor, V7ContainerCommandTarget,
    run_v7_attached_command_capture,
};
use crate::control_plane::migration::{MigrationBackup, MigrationOperationError};
use crate::control_plane::retention::{
    BackupResourceIdentity, store_backup_artifact_for_identity, verify_stored_backup_artifact,
};
use crate::control_plane::shared_infrastructure::RabbitMqProjectDefinition;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

const EXPORT_SCRIPT: &str = "set -eu\n\
    trap 'rm -f \"$STACKCTL_DEFINITIONS_FILE\"' EXIT HUP INT TERM\n\
    rabbitmqctl export_definitions --vhost=\"$STACKCTL_VHOST\" \
    \"$STACKCTL_DEFINITIONS_FILE\" >/dev/null\n\
    cat \"$STACKCTL_DEFINITIONS_FILE\"";

/// Exports and remaps one accepted empty-message vhost into v8 definitions.
pub(super) async fn backup_v7_rabbitmq_vhost(
    executor: &(impl V7ContainerCommandExecutor + Sync),
    target: &V7ContainerCommandTarget,
    source_vhost: &str,
    source_credential: &V7RabbitMqCredential,
    target_definition: &RabbitMqProjectDefinition,
    identity: &BackupResourceIdentity,
    backup_root: &Path,
    created_at_unix_seconds: i64,
    verified_at_unix_seconds: i64,
    timeout: Duration,
) -> Result<MigrationBackup, MigrationOperationError> {
    prove_v7_no_messages(executor, target, source_vhost, timeout).await?;
    let source = capture_v7_definitions(executor, target, source_vhost, timeout).await?;
    let transformed = transform_v7_rabbitmq_definitions(
        &source,
        source_vhost,
        source_credential,
        target_definition,
    )?;
    let stored = store_backup_artifact_for_identity(
        identity,
        &transformed,
        created_at_unix_seconds,
        backup_root,
    )
    .map_err(|error| operation_error("v7 RabbitMQ backup storage failed", error))?;
    let evidence = verify_stored_backup_artifact(&stored, verified_at_unix_seconds)
        .map_err(|error| operation_error("v7 RabbitMQ backup verification failed", error))?;
    let reference = stored.recovery_point().to_str().ok_or_else(|| {
        MigrationOperationError::new("v7 RabbitMQ recovery path is not valid Unicode")
    })?;
    MigrationBackup::new(
        reference,
        evidence.artifact_sha256(),
        evidence.artifact_size_bytes(),
    )
}

pub(super) async fn prove_v7_no_messages(
    executor: &(impl V7ContainerCommandExecutor + Sync),
    target: &V7ContainerCommandTarget,
    vhost: &str,
    timeout: Duration,
) -> Result<(), MigrationOperationError> {
    let request = message_inventory_request(vhost)?;
    let command = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "inventory accepted v7 RabbitMQ messages",
        timeout,
    )
    .map_err(|error| operation_error("v7 RabbitMQ message inventory is invalid", error))?;
    let output = run_v7_attached_command_capture(executor, target, &command)
        .await
        .map_err(|error| operation_error("v7 RabbitMQ message inventory failed", error))?;
    validate_no_messages(vhost, &output)
}

pub(super) async fn capture_v7_definitions(
    executor: &(impl V7ContainerCommandExecutor + Sync),
    target: &V7ContainerCommandTarget,
    vhost: &str,
    timeout: Duration,
) -> Result<Vec<u8>, MigrationOperationError> {
    let request = definitions_request(vhost)?;
    let command = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "export accepted v7 RabbitMQ definitions",
        timeout,
    )
    .map_err(|error| operation_error("v7 RabbitMQ definitions request is invalid", error))?;
    run_v7_attached_command_capture(executor, target, &command)
        .await
        .map_err(|error| operation_error("export accepted v7 RabbitMQ definitions", error))
}

pub(super) fn message_inventory_request(
    vhost: &str,
) -> Result<CommandRequest, MigrationOperationError> {
    CommandRequest::new(
        vec![
            "rabbitmqctl".to_owned(),
            "list_queues".to_owned(),
            "--vhost".to_owned(),
            vhost.to_owned(),
            "name".to_owned(),
            "messages".to_owned(),
            "--no-table-headers".to_owned(),
        ],
        BTreeMap::new(),
        None,
    )
    .map_err(|error| operation_error("RabbitMQ message inventory request is invalid", error))
}

pub(super) fn definitions_request(vhost: &str) -> Result<CommandRequest, MigrationOperationError> {
    let suffix = hex::encode(Sha256::digest(vhost.as_bytes()));
    CommandRequest::new(
        vec!["sh".to_owned(), "-c".to_owned(), EXPORT_SCRIPT.to_owned()],
        BTreeMap::from([
            (
                "STACKCTL_DEFINITIONS_FILE".to_owned(),
                format!(
                    "/tmp/.stackctl-v7-rabbitmq-{}.json",
                    suffix.get(..16).unwrap_or(&suffix)
                ),
            ),
            ("STACKCTL_VHOST".to_owned(), vhost.to_owned()),
        ]),
        None,
    )
    .map_err(|error| operation_error("RabbitMQ definitions request is invalid", error))
}

pub(super) fn validate_no_messages(
    vhost: &str,
    output: &[u8],
) -> Result<(), MigrationOperationError> {
    let output = std::str::from_utf8(output)
        .map_err(|error| operation_error("RabbitMQ message inventory is not UTF-8", error))?;
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let (queue, messages) = line.rsplit_once('\t').ok_or_else(|| {
            MigrationOperationError::new(format!(
                "RabbitMQ message inventory returned malformed row '{line}'"
            ))
        })?;
        let messages = messages.trim().parse::<u64>().map_err(|error| {
            operation_error(
                &format!("RabbitMQ queue '{queue}' returned an invalid message count"),
                error,
            )
        })?;
        if messages > 0 {
            return Err(MigrationOperationError::new(format!(
                "RabbitMQ vhost '{vhost}' contains {messages} message(s) in queue '{queue}'; \
                 definitions-only migration cannot preserve message contents"
            )));
        }
    }
    Ok(())
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
