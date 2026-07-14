use super::RabbitMqRestoreOptions;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, OwnedContainer,
    StreamingCommandOptions, run_attached_command_capture, run_streaming_command,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::retention::{
    BackupResourceIdentity, open_stored_backup_artifact, verify_stored_backup_artifact,
};
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use std::collections::BTreeMap;

const RESTORE_SCRIPT: &str = "set -eu\n\
    trap 'rm -f \"$STACKCTL_DEFINITIONS_FILE\"' EXIT HUP INT TERM\n\
    cat >\"$STACKCTL_DEFINITIONS_FILE\"\n\
    if [ \"$STACKCTL_DELETE_VHOST\" = \"1\" ]; then\n\
        rabbitmqctl delete_vhost \"$STACKCTL_VHOST\" >/dev/null\n\
    fi\n\
    rabbitmqctl import_definitions \"$STACKCTL_DEFINITIONS_FILE\" >/dev/null";

/// Replaces one exact empty vhost with verified topology definitions.
pub(crate) async fn restore_rabbitmq_vhost(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &RabbitMqRestoreOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let vhost = validate(container, options)?;
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
    let delete_vhost = vhost_exists(executor, container, &vhost, options.timeout).await?;

    let definitions_file = format!(
        "/tmp/.stackctl-rabbitmq-definitions-{vhost}-restore-{}.json",
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
            ("STACKCTL_VHOST".to_owned(), vhost.clone()),
        ]),
        None,
    )
    .map_err(|error| operation_error("RabbitMQ restore request is invalid", error))?;
    let command = StreamingCommandOptions::new(request, "restore RabbitMQ vhost", options.timeout)
        .map_err(|error| operation_error("RabbitMQ restore request is invalid", error))?;
    let mut artifact = tokio::fs::File::open(stored.artifact_file())
        .await
        .map_err(|error| operation_error("RabbitMQ restore artifact open failed", error))?;
    let mut output = tokio::io::sink();

    run_streaming_command(executor, container, &command, &mut artifact, &mut output)
        .await
        .map_err(|error| operation_error("RabbitMQ restore failed", error))?;
    if !vhost_exists(executor, container, &vhost, options.timeout).await? {
        return Err(MigrationOperationError::new(
            "RabbitMQ restore did not recreate the exact vhost",
        ));
    }

    Ok(())
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
        || credential.project_id() != Some(logical.project_id())
        || credential.service_id() != logical.service_id()
        || credential.username() != expected_username
        || credential.secret().is_empty()
        || credential.lifecycle() != CredentialLifecycle::Active
        || expected_username.len() > 128
        || vhost.len() > 128
        || container.metadata().installation_id() != options.installation_id
        || container.metadata().compatibility_fingerprint() != recovery.compatibility_fingerprint()
        || !exact_recovery;
    if invalid {
        return Err(MigrationOperationError::new(
            "RabbitMQ restore request does not match its owned recovery point",
        ));
    }

    Ok(vhost)
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
