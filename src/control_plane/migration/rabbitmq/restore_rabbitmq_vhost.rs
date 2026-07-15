use super::{RabbitMqRecoveryArtifact, RabbitMqRestoreOptions};
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, ContainerLifecycle, ContainerState,
    ContainerVolumeArchive, OwnedContainer, OwnedVolume, StreamingCommandOptions,
    run_attached_command_capture, run_streaming_command,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::retention::{
    BackupResourceIdentity, open_stored_backup_artifact, verify_stored_backup_artifact,
};
use crate::control_plane::shared_infrastructure::wait_for_rabbitmq_readiness;
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use std::collections::BTreeMap;
use std::io::Cursor;
use tokio::io::AsyncSeekExt;

use super::backup_rabbitmq_vhost::{inventory_messages, locate_message_store};
use super::validate_rabbitmq_message_store_archive::validate_rabbitmq_message_store_archive;

const RESTORE_SCRIPT: &str = "set -eu\n\
    trap 'rm -f \"$STACKCTL_DEFINITIONS_FILE\"' EXIT HUP INT TERM\n\
    cat >\"$STACKCTL_DEFINITIONS_FILE\"\n\
    if [ \"$STACKCTL_DELETE_VHOST\" = \"1\" ]; then\n\
        rabbitmqctl delete_vhost \"$STACKCTL_VHOST\" >/dev/null\n\
    fi\n\
    rabbitmqctl import_definitions \"$STACKCTL_DEFINITIONS_FILE\" >/dev/null\n\
    rabbitmqctl set_permissions --vhost \"$STACKCTL_VHOST\" \
        \"$STACKCTL_USERNAME\" '.*' '.*' '.*' >/dev/null";

/// Restores one exact vhost topology and quiesced message-store snapshot.
pub(crate) async fn restore_rabbitmq_vhost<E>(
    engine: &mut E,
    container: &OwnedContainer,
    volume: &OwnedVolume,
    options: &RabbitMqRestoreOptions<'_>,
) -> Result<(), MigrationOperationError>
where
    E: CommandExecutor + ContainerLifecycle + ContainerVolumeArchive,
{
    let vhost = validate(container, volume, options)?;
    let recovery = options.recovery_point;
    let identity =
        BackupResourceIdentity::from_logical(options.logical_resource, options.installation_id);
    let stored = open_stored_backup_artifact(recovery.reference())
        .map_err(|error| operation_error("RabbitMQ restore backup is unavailable", error))?;
    let evidence = verify_stored_backup_artifact(&stored, options.verified_at_unix_seconds)
        .map_err(|error| operation_error("RabbitMQ restore backup verification failed", error))?;
    if !evidence.matches_identity(&identity)
        || evidence.artifact_sha256() != recovery.artifact_sha256()
        || evidence.artifact_size_bytes() != recovery.artifact_size_bytes()
    {
        return Err(MigrationOperationError::new(
            "RabbitMQ restore backup does not match its recovery point",
        ));
    }
    let mut stored_artifact = tokio::fs::File::open(stored.artifact_file())
        .await
        .map_err(|error| operation_error("RabbitMQ restore artifact open failed", error))?;
    let artifact = RabbitMqRecoveryArtifact::read_from(&mut stored_artifact).await?;
    if artifact.vhost() != vhost {
        return Err(MigrationOperationError::new(format!(
            "RabbitMQ recovery artifact vhost '{}' does not match '{vhost}'",
            artifact.vhost()
        )));
    }
    let archive_offset = stored_artifact.stream_position().await.map_err(|error| {
        operation_error("RabbitMQ restore archive position inspection failed", error)
    })?;
    let stored_artifact = stored_artifact.into_std().await;
    let message_store_path = artifact.relative_message_store_path().to_path_buf();
    let expected_sha256 = evidence.artifact_sha256().to_owned();
    let expected_size_bytes = evidence.artifact_size_bytes();
    let stored_artifact = tokio::task::spawn_blocking(move || {
        validate_rabbitmq_message_store_archive(
            stored_artifact,
            archive_offset,
            &message_store_path,
            &expected_sha256,
            expected_size_bytes,
        )
    })
    .await
    .map_err(|error| operation_error("RabbitMQ archive validation task failed", error))??;
    let stored_artifact = tokio::fs::File::from_std(stored_artifact);
    let was_running = match engine
        .inspect(container)
        .await
        .map_err(|error| operation_error("RabbitMQ restore state inspection failed", error))?
    {
        ContainerState::Running => true,
        ContainerState::Stopped => {
            engine
                .start(container)
                .await
                .map_err(|error| operation_error("RabbitMQ restore broker start failed", error))?;
            if let Err(error) = wait_for_rabbitmq_readiness(engine, container).await {
                let readiness = operation_error("RabbitMQ restore broker readiness failed", error);
                let stop = engine.stop(container).await.map_err(|error| {
                    operation_error("RabbitMQ restored broker re-quiesce failed", error)
                });
                return match stop {
                    Ok(()) => Err(readiness),
                    Err(stop) => Err(MigrationOperationError::new(format!("{readiness}; {stop}"))),
                };
            }
            false
        }
        ContainerState::Missing => {
            return Err(MigrationOperationError::new(
                "RabbitMQ restore container is missing",
            ));
        }
    };
    let restore = restore_running_broker(
        engine,
        container,
        volume,
        Box::new(stored_artifact),
        &artifact,
        options,
    )
    .await;
    let restore_prior_state = if was_running {
        Ok(())
    } else {
        engine
            .stop(container)
            .await
            .map_err(|error| operation_error("RabbitMQ restored broker re-quiesce failed", error))
    };
    let mut errors = [restore.err(), restore_prior_state.err()]
        .into_iter()
        .flatten()
        .map(|error| error.to_string());
    let Some(mut detail) = errors.next() else {
        return Ok(());
    };
    for error in errors {
        detail.push_str("; ");
        detail.push_str(&error);
    }

    Err(MigrationOperationError::new(detail))
}

async fn restore_running_broker<E>(
    engine: &mut E,
    container: &OwnedContainer,
    volume: &OwnedVolume,
    message_store: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
    artifact: &RabbitMqRecoveryArtifact,
    options: &RabbitMqRestoreOptions<'_>,
) -> Result<(), MigrationOperationError>
where
    E: CommandExecutor + ContainerLifecycle + ContainerVolumeArchive,
{
    import_definitions(engine, container, artifact, options, true).await?;
    let active_path =
        locate_message_store(engine, container, artifact.vhost(), options.timeout).await?;
    if active_path != artifact.relative_message_store_path().to_string_lossy() {
        return Err(MigrationOperationError::new(format!(
            "RabbitMQ restored vhost message-store path '{active_path}' does not match recovery path '{}'",
            artifact.relative_message_store_path().display()
        )));
    }
    engine
        .stop(container)
        .await
        .map_err(|error| operation_error("RabbitMQ restore broker quiesce failed", error))?;
    let upload = engine
        .upload_volume_subpath_archive(
            container,
            volume,
            artifact.relative_message_store_path(),
            message_store,
        )
        .await
        .map_err(|error| operation_error("RabbitMQ message-store restore failed", error));
    let restart = engine
        .start(container)
        .await
        .map_err(|error| operation_error("RabbitMQ restored broker restart failed", error));
    let restart = match restart {
        Ok(()) => wait_for_rabbitmq_readiness(engine, container)
            .await
            .map_err(|error| operation_error("RabbitMQ restored broker readiness failed", error)),
        Err(error) => Err(error),
    };
    match (upload, restart) {
        (Ok(()), Ok(())) => {}
        (Err(error), Ok(())) | (Ok(()), Err(error)) => return Err(error),
        (Err(error), Err(restart)) => {
            return Err(MigrationOperationError::new(format!(
                "{error}; RabbitMQ broker restart also failed: {restart}"
            )));
        }
    }
    import_definitions(engine, container, artifact, options, false).await?;
    if !vhost_exists(engine, container, artifact.vhost(), options.timeout).await? {
        return Err(MigrationOperationError::new(
            "RabbitMQ restore did not recreate the exact vhost",
        ));
    }
    let messages = inventory_messages(engine, container, artifact.vhost(), options.timeout).await?;
    if &messages != artifact.queue_messages() {
        return Err(MigrationOperationError::new(format!(
            "RabbitMQ restored queue message counts do not match recovery metadata: expected {:?}, observed {messages:?}",
            artifact.queue_messages()
        )));
    }

    Ok(())
}

async fn import_definitions(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    artifact: &RabbitMqRecoveryArtifact,
    options: &RabbitMqRestoreOptions<'_>,
    replace_vhost: bool,
) -> Result<(), MigrationOperationError> {
    let delete_vhost = replace_vhost
        && vhost_exists(executor, container, artifact.vhost(), options.timeout).await?;
    let definitions_file = format!(
        "/tmp/.stackctl-rabbitmq-definitions-{}-restore-{}.json",
        artifact.vhost(),
        options.verified_at_unix_seconds
    );
    let request = CommandRequest::new(
        vec!["sh".to_owned(), "-c".to_owned(), RESTORE_SCRIPT.to_owned()],
        BTreeMap::from([
            (
                "STACKCTL_DELETE_VHOST".to_owned(),
                if delete_vhost { "1" } else { "0" }.to_owned(),
            ),
            ("STACKCTL_DEFINITIONS_FILE".to_owned(), definitions_file),
            (
                "STACKCTL_USERNAME".to_owned(),
                options.credential.username().to_owned(),
            ),
            ("STACKCTL_VHOST".to_owned(), artifact.vhost().to_owned()),
        ]),
        None,
    )
    .map_err(|error| operation_error("RabbitMQ restore request is invalid", error))?;
    let command = StreamingCommandOptions::new(request, "restore RabbitMQ vhost", options.timeout)
        .map_err(|error| operation_error("RabbitMQ restore request is invalid", error))?;
    let definitions = serde_json::to_vec(artifact.definitions())
        .map_err(|error| operation_error("RabbitMQ restore definitions encoding failed", error))?;
    let mut input = Cursor::new(definitions);
    let mut output = tokio::io::sink();
    run_streaming_command(executor, container, &command, &mut input, &mut output)
        .await
        .map_err(|error| operation_error("RabbitMQ definitions restore failed", error))
}

async fn vhost_exists(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    vhost: &str,
    timeout: std::time::Duration,
) -> Result<bool, MigrationOperationError> {
    let request = CommandRequest::new(
        vec![
            "rabbitmqctl".to_owned(),
            "list_vhosts".to_owned(),
            "name".to_owned(),
            "--no-table-headers".to_owned(),
        ],
        BTreeMap::new(),
        None,
    )
    .map_err(|error| operation_error("RabbitMQ vhost inventory request is invalid", error))?;
    let command = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "inventory RabbitMQ vhosts for restore",
        timeout,
    )
    .map_err(|error| operation_error("RabbitMQ vhost inventory request is invalid", error))?;
    let output = run_attached_command_capture(executor, container, &command)
        .await
        .map_err(|error| operation_error("RabbitMQ vhost inventory failed", error))?;
    let output = std::str::from_utf8(&output)
        .map_err(|error| operation_error("RabbitMQ vhost inventory is not UTF-8", error))?;

    Ok(output.lines().map(str::trim).any(|name| name == vhost))
}

fn validate(
    container: &OwnedContainer,
    volume: &OwnedVolume,
    options: &RabbitMqRestoreOptions<'_>,
) -> Result<String, MigrationOperationError> {
    let recovery = options.recovery_point;
    let logical = options.logical_resource;
    let credential = options.credential;
    let identity = format!(
        "{}_{}",
        logical.project_id().replace('-', "_"),
        logical.service_id().replace('-', "_")
    );
    let expected_username = format!("st_{identity}");
    let vhost = format!("stackctl_{identity}");
    let exact_recovery = recovery.project_id() == logical.project_id()
        && recovery.service_id() == logical.service_id()
        && recovery.logical_resource_id() == logical.logical_resource_id()
        && recovery.resource_kind() == logical.kind()
        && recovery.compatibility_fingerprint() == logical.compatibility_fingerprint();
    let invalid = options.installation_id.is_empty()
        || options.timeout.is_zero()
        || options.verified_at_unix_seconds < recovery.verified_at_unix_seconds()
        || logical.kind() != "rabbitmq_vhost_user"
        || logical.lifecycle() != ResourceLifecycle::Active
        || logical.logical_resource_id() != credential.credential_id()
        || logical.shared_resource_id() != volume.name()
        || credential.project_id() != Some(logical.project_id())
        || credential.service_id() != logical.service_id()
        || credential.username() != expected_username
        || credential.secret().is_empty()
        || credential.lifecycle() != CredentialLifecycle::Active
        || expected_username.len() > 128
        || vhost.len() > 128
        || container.metadata().installation_id() != options.installation_id
        || volume.metadata().installation_id() != options.installation_id
        || container.metadata().project_id().is_some()
        || volume.metadata().project_id().is_some()
        || container.metadata().resource_id() != volume.metadata().resource_id()
        || container.metadata().compatibility_fingerprint() != recovery.compatibility_fingerprint()
        || volume.metadata().compatibility_fingerprint() != recovery.compatibility_fingerprint()
        || !exact_recovery;
    if invalid {
        return Err(MigrationOperationError::new(
            "RabbitMQ restore request does not match its owned recovery point and volume",
        ));
    }

    Ok(vhost)
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
