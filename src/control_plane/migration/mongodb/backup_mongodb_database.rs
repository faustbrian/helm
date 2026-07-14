use super::{MongoDbBackupOptions, mongodb_connection_uri::mongodb_connection_uri};
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
const DUMP_SCRIPT: &str = "set -eu\nexec mongodump --uri=\"$STACKCTL_MONGODB_URI\" --archive \
    --db=\"$STACKCTL_MONGODB_DATABASE\"";

/// Streams one MongoDB archive into a verified logical recovery point.
pub(crate) async fn backup_mongodb_database(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &MongoDbBackupOptions<'_>,
) -> Result<MigrationBackup, MigrationOperationError> {
    validate(container, options)?;
    let request = CommandRequest::new(
        vec!["sh".to_owned(), "-c".to_owned(), DUMP_SCRIPT.to_owned()],
        BTreeMap::from([
            (
                "STACKCTL_MONGODB_URI".to_owned(),
                mongodb_connection_uri(
                    options.credential.username(),
                    options.credential.secret(),
                    options.database_name,
                    options.database_name,
                ),
            ),
            (
                "STACKCTL_MONGODB_DATABASE".to_owned(),
                options.database_name.to_owned(),
            ),
        ]),
        None,
    )
    .map_err(|error| operation_error("MongoDB backup request is invalid", error))?;
    let command = StreamingCommandOptions::new(request, "dump MongoDB database", options.timeout)
        .map_err(|error| operation_error("MongoDB backup request is invalid", error))?;
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
        result.map_err(|error| operation_error("MongoDB backup failed", error))?;
        close.map_err(|error| operation_error("MongoDB backup output close failed", error))
    };
    let store = async {
        store_backup_artifact_from_async_reader(
            &identity,
            &mut backup_reader,
            options.created_at_unix_seconds,
            options.backup_root,
        )
        .await
        .map_err(|error| operation_error("MongoDB backup storage failed", error))
    };
    let (_, stored) = futures_util::future::try_join(dump, store).await?;
    let evidence = verify_stored_backup_artifact(&stored, options.created_at_unix_seconds)
        .map_err(|error| operation_error("MongoDB backup verification failed", error))?;
    let reference = stored.recovery_point().to_str().ok_or_else(|| {
        MigrationOperationError::new("MongoDB backup recovery point is not valid Unicode")
    })?;

    MigrationBackup::new(
        reference,
        evidence.artifact_sha256(),
        evidence.artifact_size_bytes(),
    )
}

fn validate(
    container: &OwnedContainer,
    options: &MongoDbBackupOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let invalid = options.database_name.is_empty()
        || options.installation_id.is_empty()
        || !options.backup_root.is_absolute()
        || options.backup_root.to_str().is_none()
        || options.credential.project_id() != Some(options.logical_resource.project_id())
        || options.credential.service_id() != options.logical_resource.service_id()
        || options.credential.username().is_empty()
        || options.credential.secret().is_empty()
        || options.timeout.is_zero()
        || options.logical_resource.kind() != "mongodb_database"
        || options.logical_resource.lifecycle() != ResourceLifecycle::Active
        || options.credential.lifecycle() != CredentialLifecycle::Active
        || container.metadata().installation_id() != options.installation_id
        || container.metadata().compatibility_fingerprint()
            != options.logical_resource.compatibility_fingerprint();
    if invalid {
        return Err(MigrationOperationError::new(
            "MongoDB backup request does not match an active owned logical resource",
        ));
    }

    Ok(())
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
