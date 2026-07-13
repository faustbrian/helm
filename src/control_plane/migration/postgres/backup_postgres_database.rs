use super::PostgresBackupOptions;
use crate::control_plane::engine::{
    CommandExecutor, CommandRequest, OwnedContainer, StreamingCommandOptions, run_streaming_command,
};
use crate::control_plane::migration::{MigrationBackup, MigrationOperationError};
use crate::control_plane::retention::{
    BackupResourceIdentity, store_backup_artifact_from_async_reader, verify_stored_backup_artifact,
};
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use std::collections::BTreeMap;
use tokio::io::{AsyncWriteExt, duplex};

const STREAM_BUFFER_BYTES: usize = 64 * 1024;

/// Streams a custom-format dump into a verified logical recovery point.
pub(crate) async fn backup_postgres_database(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &PostgresBackupOptions<'_>,
) -> Result<MigrationBackup, MigrationOperationError> {
    validate(container, options)?;
    let request = CommandRequest::new(
        vec![
            "pg_dump".to_owned(),
            "--format=custom".to_owned(),
            "--no-owner".to_owned(),
            "--no-privileges".to_owned(),
            format!("--username={}", options.credential.username()),
            format!("--dbname={}", options.database_name),
        ],
        BTreeMap::from([(
            "PGPASSWORD".to_owned(),
            options.credential.secret().to_owned(),
        )]),
        None,
    )
    .map_err(|error| operation_error("PostgreSQL backup request is invalid", error))?;
    let command =
        StreamingCommandOptions::new(request, "dump PostgreSQL database", options.timeout)
            .map_err(|error| operation_error("PostgreSQL backup request is invalid", error))?;
    let identity =
        BackupResourceIdentity::from_logical(options.logical_resource, options.installation_id);
    let (mut backup_reader, mut command_output) = duplex(STREAM_BUFFER_BYTES);
    let mut command_input = tokio::io::empty();
    let dump = async {
        let result = run_streaming_command(
            executor,
            container,
            &command,
            &mut command_input,
            &mut command_output,
        )
        .await;
        let close = command_output.shutdown().await;
        result.map_err(|error| operation_error("PostgreSQL backup failed", error))?;
        close.map_err(|error| operation_error("PostgreSQL backup output close failed", error))
    };
    let store = async {
        store_backup_artifact_from_async_reader(
            &identity,
            &mut backup_reader,
            options.created_at_unix_seconds,
            options.backup_root,
        )
        .await
        .map_err(|error| operation_error("PostgreSQL backup storage failed", error))
    };
    let (_, stored) = futures_util::future::try_join(dump, store).await?;
    let evidence = verify_stored_backup_artifact(&stored, options.created_at_unix_seconds)
        .map_err(|error| operation_error("PostgreSQL backup verification failed", error))?;
    let reference = stored.recovery_point().to_str().ok_or_else(|| {
        MigrationOperationError::new("PostgreSQL backup recovery point is not valid Unicode")
    })?;

    MigrationBackup::new(
        reference,
        evidence.artifact_sha256(),
        evidence.artifact_size_bytes(),
    )
}

fn validate(
    container: &OwnedContainer,
    options: &PostgresBackupOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let invalid = options.database_name.is_empty()
        || options.installation_id.is_empty()
        || !options.backup_root.is_absolute()
        || options.backup_root.to_str().is_none()
        || options.credential.username().is_empty()
        || options.credential.secret().is_empty()
        || options.timeout.is_zero()
        || options.logical_resource.kind() != "postgres_database_and_role"
        || options.logical_resource.lifecycle() != ResourceLifecycle::Active
        || options.credential.lifecycle() != CredentialLifecycle::Active
        || container.metadata().installation_id() != options.installation_id
        || container.metadata().compatibility_fingerprint()
            != options.logical_resource.compatibility_fingerprint();
    if invalid {
        return Err(MigrationOperationError::new(
            "PostgreSQL backup request does not match an active owned logical resource",
        ));
    }

    Ok(())
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
