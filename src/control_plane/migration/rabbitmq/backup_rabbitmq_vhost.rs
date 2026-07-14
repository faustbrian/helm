use super::{RabbitMqBackupOptions, RabbitMqRecoveryArtifact};
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, ContainerLifecycle, ContainerState,
    ContainerVolumeArchive, OwnedContainer, OwnedVolume, run_attached_command_capture,
};
use crate::control_plane::migration::{MigrationBackup, MigrationOperationError};
use crate::control_plane::retention::{
    BackupResourceIdentity, store_backup_artifact_from_async_reader, verify_stored_backup_artifact,
};
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use std::collections::BTreeMap;
use tokio::io::{AsyncWriteExt, duplex};

const STREAM_BUFFER_BYTES: usize = 64 * 1_024;
const EXPORT_SCRIPT: &str = "set -eu\n\
    trap 'rm -f \"$STACKCTL_DEFINITIONS_FILE\"' EXIT HUP INT TERM\n\
    rabbitmqctl export_definitions --vhost=\"$STACKCTL_VHOST\" \
    \"$STACKCTL_DEFINITIONS_FILE\" >/dev/null\n\
    cat \"$STACKCTL_DEFINITIONS_FILE\"";
const LOCATE_MESSAGE_STORE_SCRIPT: &str = "set -eu\n\
    root=/var/lib/rabbitmq/mnesia/rabbit@localhost/msg_stores/vhosts\n\
    match=\n\
    for marker in \"$root\"/*/.vhost; do\n\
        [ -f \"$marker\" ] || continue\n\
        if [ \"$(cat \"$marker\")\" = \"$STACKCTL_VHOST\" ]; then\n\
            [ -z \"$match\" ] || exit 65\n\
            match=${marker%/.vhost}\n\
        fi\n\
    done\n\
    [ -n \"$match\" ] || exit 66\n\
    printf '%s\\n' \"${match#/var/lib/rabbitmq/}\"";

/// Quiesces one broker and archives an exact vhost's topology and messages.
pub(crate) async fn backup_rabbitmq_vhost<E>(
    engine: &mut E,
    container: &OwnedContainer,
    volume: &OwnedVolume,
    options: &RabbitMqBackupOptions<'_>,
) -> Result<MigrationBackup, MigrationOperationError>
where
    E: CommandExecutor + ContainerLifecycle + ContainerVolumeArchive,
{
    let vhost = validate(container, volume, options)?;
    let was_running = match engine
        .inspect(container)
        .await
        .map_err(|error| operation_error("RabbitMQ backup state inspection failed", error))?
    {
        ContainerState::Running => true,
        ContainerState::Stopped => {
            engine
                .start(container)
                .await
                .map_err(|error| operation_error("RabbitMQ backup broker start failed", error))?;
            false
        }
        ContainerState::Missing => {
            return Err(MigrationOperationError::new(
                "RabbitMQ backup container is missing",
            ));
        }
    };
    let prepared = prepare_recovery_artifact(engine, container, &vhost, options).await;
    let artifact = match prepared {
        Ok(artifact) => artifact,
        Err(error) if was_running => {
            let resume = resume_listeners(engine, container, options.timeout).await;
            return match resume {
                Ok(()) => Err(error),
                Err(resume) => Err(MigrationOperationError::new(format!("{error}; {resume}"))),
            };
        }
        Err(error) => {
            let stop = engine
                .stop(container)
                .await
                .map_err(|stop| operation_error("RabbitMQ backup broker re-quiesce failed", stop));
            return match stop {
                Ok(()) => Err(error),
                Err(stop) => Err(MigrationOperationError::new(format!("{error}; {stop}"))),
            };
        }
    };
    if let Err(error) = engine.stop(container).await {
        let error = operation_error("RabbitMQ backup broker quiesce failed", error);
        let cleanup = if was_running {
            resume_listeners(engine, container, options.timeout).await
        } else {
            engine.stop(container).await.map_err(|cleanup| {
                operation_error("RabbitMQ backup broker re-quiesce failed", cleanup)
            })
        };
        return match cleanup {
            Ok(()) => Err(error),
            Err(cleanup) => Err(MigrationOperationError::new(format!("{error}; {cleanup}"))),
        };
    }
    let backup = stream_backup(engine, container, volume, &artifact, options).await;
    let restart = if was_running {
        engine
            .start(container)
            .await
            .map_err(|error| operation_error("RabbitMQ backup broker restart failed", error))
    } else {
        Ok(())
    };

    match (backup, restart) {
        (Ok(backup), Ok(())) => Ok(backup),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(restart)) => Err(MigrationOperationError::new(format!(
            "{error}; RabbitMQ broker restart also failed: {restart}"
        ))),
    }
}

async fn prepare_recovery_artifact(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    vhost: &str,
    options: &RabbitMqBackupOptions<'_>,
) -> Result<RabbitMqRecoveryArtifact, MigrationOperationError> {
    quiesce_client_activity(executor, container, options.timeout).await?;
    let queue_messages = inventory_messages(executor, container, vhost, options.timeout).await?;
    let definitions = export_definitions(executor, container, vhost, options).await?;
    let relative_message_store_path =
        locate_message_store(executor, container, vhost, options.timeout).await?;

    RabbitMqRecoveryArtifact::new(
        vhost.to_owned(),
        relative_message_store_path,
        definitions,
        queue_messages,
    )
}

async fn quiesce_client_activity(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    timeout: std::time::Duration,
) -> Result<(), MigrationOperationError> {
    run_capture(
        executor,
        container,
        vec!["rabbitmqctl".to_owned(), "suspend_listeners".to_owned()],
        BTreeMap::new(),
        "suspend RabbitMQ listeners for backup",
        timeout,
    )
    .await?;
    run_capture(
        executor,
        container,
        vec![
            "rabbitmqctl".to_owned(),
            "close_all_connections".to_owned(),
            "--global".to_owned(),
            "Stackctl quiesced backup".to_owned(),
        ],
        BTreeMap::new(),
        "close RabbitMQ connections for backup",
        timeout,
    )
    .await?;

    Ok(())
}

async fn resume_listeners(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    timeout: std::time::Duration,
) -> Result<(), MigrationOperationError> {
    run_capture(
        executor,
        container,
        vec!["rabbitmqctl".to_owned(), "resume_listeners".to_owned()],
        BTreeMap::new(),
        "resume RabbitMQ listeners after failed backup",
        timeout,
    )
    .await
    .map(|_| ())
}

pub(super) async fn inventory_messages(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    vhost: &str,
    timeout: std::time::Duration,
) -> Result<BTreeMap<String, u64>, MigrationOperationError> {
    let output = run_capture(
        executor,
        container,
        vec![
            "rabbitmqctl".to_owned(),
            "list_queues".to_owned(),
            "--vhost".to_owned(),
            vhost.to_owned(),
            "name".to_owned(),
            "messages".to_owned(),
            "messages_persistent".to_owned(),
            "durable".to_owned(),
            "type".to_owned(),
            "--no-table-headers".to_owned(),
        ],
        BTreeMap::new(),
        "inventory RabbitMQ vhost messages",
        timeout,
    )
    .await?;
    let output = std::str::from_utf8(&output)
        .map_err(|error| operation_error("RabbitMQ message inventory is not UTF-8", error))?;

    parse_message_inventory(output)
}

fn parse_message_inventory(output: &str) -> Result<BTreeMap<String, u64>, MigrationOperationError> {
    let mut messages_by_queue = BTreeMap::new();
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let columns = line.split('\t').collect::<Vec<_>>();
        let [queue, messages, persistent_messages, durable, queue_type] = columns.as_slice() else {
            return Err(MigrationOperationError::new(format!(
                "RabbitMQ message inventory returned malformed row '{line}'"
            )));
        };
        let queue = queue.trim();
        let messages = messages.trim().parse::<u64>().map_err(|error| {
            operation_error(
                &format!("RabbitMQ queue '{queue}' returned an invalid message count"),
                error,
            )
        })?;
        let persistent_messages = persistent_messages.trim().parse::<u64>().map_err(|error| {
            operation_error(
                &format!("RabbitMQ queue '{queue}' returned an invalid persistent message count"),
                error,
            )
        })?;
        let durable = match durable.trim() {
            "true" => true,
            "false" => false,
            value => {
                return Err(MigrationOperationError::new(format!(
                    "RabbitMQ queue '{queue}' returned invalid durable state '{value}'"
                )));
            }
        };
        if messages > 0 && messages != persistent_messages {
            return Err(MigrationOperationError::new(format!(
                "RabbitMQ queue '{queue}' contains {} non-persistent message(s); stopping the broker cannot preserve them",
                messages.saturating_sub(persistent_messages)
            )));
        }
        if messages > 0 && queue_type.trim() != "classic" {
            return Err(MigrationOperationError::new(format!(
                "RabbitMQ queue '{queue}' uses type '{}' with {messages} message(s); tenant-scoped message-store backup currently requires classic queues",
                queue_type.trim()
            )));
        }
        if !durable {
            return Err(MigrationOperationError::new(format!(
                "RabbitMQ queue '{queue}' is non-durable with {messages} message(s); its topology cannot survive the required broker restart"
            )));
        }
        if queue.is_empty()
            || messages_by_queue
                .insert(queue.to_owned(), messages)
                .is_some()
        {
            return Err(MigrationOperationError::new(format!(
                "RabbitMQ message inventory returned duplicate or empty queue '{queue}'"
            )));
        }
    }

    Ok(messages_by_queue)
}

async fn export_definitions(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    vhost: &str,
    options: &RabbitMqBackupOptions<'_>,
) -> Result<serde_json::Value, MigrationOperationError> {
    let definitions_file = format!(
        "/tmp/.stackctl-rabbitmq-definitions-{}-{}.json",
        vhost, options.created_at_unix_seconds
    );
    let output = run_capture(
        executor,
        container,
        vec!["sh".to_owned(), "-c".to_owned(), EXPORT_SCRIPT.to_owned()],
        BTreeMap::from([
            ("STACKCTL_DEFINITIONS_FILE".to_owned(), definitions_file),
            ("STACKCTL_VHOST".to_owned(), vhost.to_owned()),
        ]),
        "export RabbitMQ vhost definitions",
        options.timeout,
    )
    .await?;

    serde_json::from_slice(&output)
        .map_err(|error| operation_error("RabbitMQ definitions export is invalid JSON", error))
}

pub(super) async fn locate_message_store(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    vhost: &str,
    timeout: std::time::Duration,
) -> Result<String, MigrationOperationError> {
    let output = run_capture(
        executor,
        container,
        vec![
            "sh".to_owned(),
            "-c".to_owned(),
            LOCATE_MESSAGE_STORE_SCRIPT.to_owned(),
        ],
        BTreeMap::from([("STACKCTL_VHOST".to_owned(), vhost.to_owned())]),
        "locate RabbitMQ vhost message store",
        timeout,
    )
    .await?;
    let path = std::str::from_utf8(&output)
        .map_err(|error| operation_error("RabbitMQ message-store path is not UTF-8", error))?
        .trim();

    Ok(path.to_owned())
}

async fn run_capture(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    arguments: Vec<String>,
    environment: BTreeMap<String, String>,
    action: &'static str,
    timeout: std::time::Duration,
) -> Result<Vec<u8>, MigrationOperationError> {
    let request = CommandRequest::new(arguments, environment, None)
        .map_err(|error| operation_error("RabbitMQ backup request is invalid", error))?;
    let command = AttachedCommandOptions::new(request, Vec::new(), action, timeout)
        .map_err(|error| operation_error("RabbitMQ backup request is invalid", error))?;
    run_attached_command_capture(executor, container, &command)
        .await
        .map_err(|error| operation_error(action, error))
}

async fn stream_backup(
    engine: &impl ContainerVolumeArchive,
    container: &OwnedContainer,
    volume: &OwnedVolume,
    artifact: &RabbitMqRecoveryArtifact,
    options: &RabbitMqBackupOptions<'_>,
) -> Result<MigrationBackup, MigrationOperationError> {
    let identity =
        BackupResourceIdentity::from_logical(options.logical_resource, options.installation_id);
    let (mut backup_reader, mut archive_output) = duplex(STREAM_BUFFER_BYTES);
    let download = async {
        artifact.write_to(&mut archive_output).await?;
        let result = engine
            .download_volume_subpath_archive(
                container,
                volume,
                artifact.relative_message_store_path(),
                &mut archive_output,
            )
            .await;
        let close = archive_output.shutdown().await;
        result.map_err(|error| operation_error("RabbitMQ message-store archive failed", error))?;
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
    let (_, stored) = tokio::time::timeout(
        options.timeout,
        futures_util::future::try_join(download, store),
    )
    .await
    .map_err(|_| MigrationOperationError::new("RabbitMQ backup timed out"))??;
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

fn validate(
    container: &OwnedContainer,
    volume: &OwnedVolume,
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
        || container.metadata().compatibility_fingerprint() != logical.compatibility_fingerprint()
        || volume.metadata().compatibility_fingerprint() != logical.compatibility_fingerprint();
    if invalid {
        return Err(MigrationOperationError::new(
            "RabbitMQ backup request does not match an active owned logical resource and volume",
        ));
    }

    Ok(vhost)
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::parse_message_inventory;

    #[test]
    fn message_inventory_rejects_non_persistent_messages() {
        let error = parse_message_inventory("jobs\t2\t1\ttrue\tclassic\n")
            .expect_err("non-persistent messages must fail closed");

        assert!(error.to_string().contains("1 non-persistent message(s)"));
    }

    #[test]
    fn message_inventory_rejects_non_empty_non_classic_queues() {
        for queue_type in ["quorum", "stream"] {
            let error = parse_message_inventory(&format!("jobs\t2\t2\ttrue\t{queue_type}\n"))
                .expect_err("non-empty non-classic queues must fail closed");

            assert!(error.to_string().contains(queue_type));
            assert!(error.to_string().contains("requires classic queues"));
        }
    }

    #[test]
    fn message_inventory_rejects_non_durable_queues_at_any_depth() {
        for messages in [0, 2] {
            let error =
                parse_message_inventory(&format!("jobs\t{messages}\t{messages}\tfalse\tclassic\n"))
                    .expect_err("non-durable queues must fail closed");

            assert!(error.to_string().contains("is non-durable"));
        }
    }
}
