use super::RabbitMqBackupOptions;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, OwnedContainer,
    StreamingCommandOptions, run_attached_command_capture, run_streaming_command,
};
use crate::control_plane::migration::{MigrationBackup, MigrationOperationError};
use crate::control_plane::retention::{
    BackupResourceIdentity, store_backup_artifact_from_async_reader, verify_stored_backup_artifact,
};
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use std::collections::BTreeMap;
use tokio::io::{AsyncWriteExt, duplex};

const STREAM_BUFFER_BYTES: usize = 64 * 1024;
const EXPORT_SCRIPT: &str = "set -eu\n\
    trap 'rm -f \"$STACKCTL_DEFINITIONS_FILE\"' EXIT HUP INT TERM\n\
    rabbitmqctl export_definitions --vhost=\"$STACKCTL_VHOST\" \
    \"$STACKCTL_DEFINITIONS_FILE\" >/dev/null\n\
    cat \"$STACKCTL_DEFINITIONS_FILE\"";

/// Exports one empty vhost's complete topology into an immutable recovery point.
pub(crate) async fn backup_rabbitmq_vhost(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &RabbitMqBackupOptions<'_>,
) -> Result<MigrationBackup, MigrationOperationError> {
    let vhost = validate(container, options)?;
    prove_no_messages(executor, container, &vhost, options.timeout).await?;

    let definitions_file = format!(
        "/tmp/.stackctl-rabbitmq-definitions-{}.json",
        options.created_at_unix_seconds
    );
    let request = CommandRequest::new(
        vec!["sh".to_owned(), "-c".to_owned(), EXPORT_SCRIPT.to_owned()],
        BTreeMap::from([
            ("STACKCTL_DEFINITIONS_FILE".to_owned(), definitions_file),
            ("STACKCTL_VHOST".to_owned(), vhost),
        ]),
        None,
    )
    .map_err(|error| operation_error("RabbitMQ backup request is invalid", error))?;
    let command = StreamingCommandOptions::new(request, "export RabbitMQ vhost", options.timeout)
        .map_err(|error| operation_error("RabbitMQ backup request is invalid", error))?;
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
        result.map_err(|error| operation_error("RabbitMQ backup failed", error))?;
        close.map_err(|error| operation_error("RabbitMQ backup output close failed", error))
    };
    let store = async {
        store_backup_artifact_from_async_reader(
            &identity,
            &mut backup_reader,
            options.created_at_unix_seconds,
            options.backup_root,
        )
        .await
        .map_err(|error| operation_error("RabbitMQ backup storage failed", error))
    };
    let (_, stored) = futures_util::future::try_join(export, store).await?;
    let evidence = verify_stored_backup_artifact(&stored, options.created_at_unix_seconds)
        .map_err(|error| operation_error("RabbitMQ backup verification failed", error))?;
    let reference = stored.recovery_point().to_str().ok_or_else(|| {
        MigrationOperationError::new("RabbitMQ backup recovery point is not valid Unicode")
    })?;

    MigrationBackup::new(
        reference,
        evidence.artifact_sha256(),
        evidence.artifact_size_bytes(),
    )
}

async fn prove_no_messages(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    vhost: &str,
    timeout: std::time::Duration,
) -> Result<(), MigrationOperationError> {
    let request = CommandRequest::new(
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
    .map_err(|error| operation_error("RabbitMQ message inventory request is invalid", error))?;
    let command = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "inventory RabbitMQ vhost messages",
        timeout,
    )
    .map_err(|error| operation_error("RabbitMQ message inventory request is invalid", error))?;
    let output = run_attached_command_capture(executor, container, &command)
        .await
        .map_err(|error| operation_error("RabbitMQ message inventory failed", error))?;
    let output = std::str::from_utf8(&output)
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
                 definitions-only backup cannot preserve message contents"
            )));
        }
    }

    Ok(())
}

fn validate(
    container: &OwnedContainer,
    options: &RabbitMqBackupOptions<'_>,
) -> Result<String, MigrationOperationError> {
    let logical = options.logical_resource;
    let credential = options.credential;
    let identity = format!(
        "{}_{}",
        logical.project_id().replace('-', "_"),
        logical.service_id().replace('-', "_")
    );
    let expected_username = format!("st_{identity}");
    let vhost = format!("stackctl_{identity}");
    let invalid = options.installation_id.is_empty()
        || options.created_at_unix_seconds < 0
        || !options.backup_root.is_absolute()
        || options.backup_root.to_str().is_none()
        || options.timeout.is_zero()
        || logical.kind() != "rabbitmq_vhost_user"
        || logical.lifecycle() != ResourceLifecycle::Active
        || logical.logical_resource_id() != credential.credential_id()
        || credential.project_id() != Some(logical.project_id())
        || credential.service_id() != logical.service_id()
        || credential.username() != expected_username
        || credential.secret().is_empty()
        || credential.lifecycle() != CredentialLifecycle::Active
        || expected_username.len() > 128
        || vhost.len() > 128
        || container.metadata().installation_id() != options.installation_id
        || container.metadata().compatibility_fingerprint() != logical.compatibility_fingerprint();
    if invalid {
        return Err(MigrationOperationError::new(
            "RabbitMQ backup request does not match an active owned logical resource",
        ));
    }

    Ok(vhost)
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
