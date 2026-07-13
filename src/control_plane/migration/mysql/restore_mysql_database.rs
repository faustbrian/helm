use super::MySqlRestoreOptions;
use crate::control_plane::engine::{
    CommandExecutor, CommandRequest, OwnedContainer, StreamingCommandOptions, run_streaming_command,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::retention::{
    BackupResourceIdentity, open_stored_backup_artifact, verify_stored_backup_artifact,
};
use crate::control_plane::shared_infrastructure::MySqlFlavor;
use crate::control_plane::state::{CredentialLifecycle, MigrationPhase, ResourceLifecycle};
use std::collections::BTreeMap;

/// Restores one exact journaled dump into an isolated MySQL-family target.
pub(crate) async fn restore_mysql_database(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &MySqlRestoreOptions<'_>,
) -> Result<(), MigrationOperationError> {
    validate(container, options)?;
    let checkpoint = options.checkpoint;
    let reference = required(
        checkpoint.backup_reference(),
        "MySQL-family restore checkpoint has no backup reference",
    )?;
    let expected_checksum = required(
        checkpoint.backup_artifact_sha256(),
        "MySQL-family restore checkpoint has no backup checksum",
    )?;
    let expected_size = checkpoint.backup_artifact_size_bytes().ok_or_else(|| {
        MigrationOperationError::new("MySQL-family restore checkpoint has no backup size")
    })?;
    let identity = BackupResourceIdentity::from_logical(
        options.source_logical_resource,
        options.installation_id,
    );
    let stored = open_stored_backup_artifact(reference)
        .map_err(|error| operation_error("MySQL-family restore backup is unavailable", error))?;
    let evidence = verify_stored_backup_artifact(&stored, options.verified_at_unix_seconds)
        .map_err(|error| {
            operation_error("MySQL-family restore backup verification failed", error)
        })?;
    if !evidence.matches_identity(&identity)
        || evidence.artifact_sha256() != expected_checksum
        || evidence.artifact_size_bytes() != expected_size
    {
        return Err(MigrationOperationError::new(
            "MySQL-family restore backup does not match its durable checkpoint",
        ));
    }

    let request = CommandRequest::new(
        vec![
            client_executable(options.flavor).to_owned(),
            "--protocol=socket".to_owned(),
            format!("--user={}", options.credential.username()),
            format!("--database={}", options.target_database_name),
            "--binary-mode".to_owned(),
        ],
        BTreeMap::from([(
            "MYSQL_PWD".to_owned(),
            options.credential.secret().to_owned(),
        )]),
        None,
    )
    .map_err(|error| operation_error("MySQL-family restore request is invalid", error))?;
    let command =
        StreamingCommandOptions::new(request, "restore MySQL-family database", options.timeout)
            .map_err(|error| operation_error("MySQL-family restore request is invalid", error))?;
    let mut artifact = tokio::fs::File::open(stored.artifact_file())
        .await
        .map_err(|error| operation_error("MySQL-family restore artifact open failed", error))?;
    let mut output = tokio::io::sink();

    run_streaming_command(executor, container, &command, &mut artifact, &mut output)
        .await
        .map_err(|error| operation_error("MySQL-family restore failed", error))
}

fn validate(
    container: &OwnedContainer,
    options: &MySqlRestoreOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let checkpoint = options.checkpoint;
    let expected_kind = match options.flavor {
        MySqlFlavor::MySql => "mysql_database",
        MySqlFlavor::MariaDb => "mariadb_database",
    };
    let invalid = checkpoint.phase() != MigrationPhase::TargetProvisioned
        || options.installation_id.is_empty()
        || options.target_database_name.is_empty()
        || checkpoint.target_resource_id() != Some(options.target_database_name)
        || options.source_logical_resource.kind() != expected_kind
        || options.source_logical_resource.project_id() != checkpoint.project_id()
        || options.source_logical_resource.lifecycle() != ResourceLifecycle::Active
        || options.source_logical_resource.compatibility_fingerprint()
            != checkpoint.source_compatibility_fingerprint()
        || options.credential.username().is_empty()
        || options.credential.project_id() != Some(checkpoint.project_id())
        || options.credential.service_id() != options.source_logical_resource.service_id()
        || options.credential.secret().is_empty()
        || options.credential.lifecycle() != CredentialLifecycle::Active
        || options.timeout.is_zero()
        || options.verified_at_unix_seconds < checkpoint.updated_at_unix_seconds()
        || container.metadata().installation_id() != options.installation_id
        || container.metadata().compatibility_fingerprint()
            != checkpoint.target_compatibility_fingerprint();
    if invalid {
        return Err(MigrationOperationError::new(
            "MySQL-family restore request does not match its owned migration target",
        ));
    }

    Ok(())
}

const fn client_executable(flavor: MySqlFlavor) -> &'static str {
    match flavor {
        MySqlFlavor::MySql => "mysql",
        MySqlFlavor::MariaDb => "mariadb",
    }
}

fn required<'value>(
    value: Option<&'value str>,
    detail: &str,
) -> Result<&'value str, MigrationOperationError> {
    value.ok_or_else(|| MigrationOperationError::new(detail))
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
