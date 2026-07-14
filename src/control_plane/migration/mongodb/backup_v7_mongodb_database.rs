use super::{V7MongoDbCredential, mongodb_connection_uri::mongodb_connection_uri};
use crate::control_plane::engine::{
    CommandRequest, StreamingCommandOptions, V7ContainerCommandExecutor, V7ContainerCommandTarget,
    run_v7_streaming_command,
};
use crate::control_plane::migration::{MigrationBackup, MigrationOperationError};
use crate::control_plane::retention::{
    BackupResourceIdentity, store_backup_artifact_from_async_reader, verify_stored_backup_artifact,
};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncWriteExt, duplex};

const STREAM_BUFFER_BYTES: usize = 64 * 1024;
const DUMP_SCRIPT: &str = "set -eu\nexec mongodump --uri=\"$STACKCTL_MONGODB_URI\" --archive --db=\"$STACKCTL_MONGODB_DATABASE\"";

/// Streams one accepted v7 MongoDB archive into verified private recovery.
pub(super) async fn backup_v7_mongodb_database(
    executor: &(impl V7ContainerCommandExecutor + Sync),
    target: &V7ContainerCommandTarget,
    credential: &V7MongoDbCredential,
    database_name: &str,
    identity: &BackupResourceIdentity,
    backup_root: &Path,
    created_at_unix_seconds: i64,
    verified_at_unix_seconds: i64,
    timeout: Duration,
) -> Result<MigrationBackup, MigrationOperationError> {
    let request = backup_request(credential, database_name)?;
    let command =
        StreamingCommandOptions::new(request, "dump accepted v7 MongoDB database", timeout)
            .map_err(|error| operation_error("v7 MongoDB backup request is invalid", error))?;
    let (mut backup_reader, mut command_output) = duplex(STREAM_BUFFER_BYTES);
    let mut command_input = tokio::io::empty();
    let dump = async {
        let result = run_v7_streaming_command(
            executor,
            target,
            &command,
            &mut command_input,
            &mut command_output,
        )
        .await;
        let close = command_output.shutdown().await;
        result.map_err(|error| operation_error("v7 MongoDB backup failed", error))?;
        close.map_err(|error| operation_error("v7 MongoDB backup close failed", error))
    };
    let store = async {
        store_backup_artifact_from_async_reader(
            identity,
            &mut backup_reader,
            created_at_unix_seconds,
            backup_root,
        )
        .await
        .map_err(|error| operation_error("v7 MongoDB backup storage failed", error))
    };
    let (_, stored) = futures_util::future::try_join(dump, store).await?;
    let evidence = verify_stored_backup_artifact(&stored, verified_at_unix_seconds)
        .map_err(|error| operation_error("v7 MongoDB backup verification failed", error))?;
    let reference = stored.recovery_point().to_str().ok_or_else(|| {
        MigrationOperationError::new("v7 MongoDB recovery path is not valid Unicode")
    })?;

    MigrationBackup::new(
        reference,
        evidence.artifact_sha256(),
        evidence.artifact_size_bytes(),
    )
}

pub(super) fn backup_request(
    credential: &V7MongoDbCredential,
    database_name: &str,
) -> Result<CommandRequest, MigrationOperationError> {
    CommandRequest::new(
        vec!["sh".to_owned(), "-c".to_owned(), DUMP_SCRIPT.to_owned()],
        BTreeMap::from([
            (
                "STACKCTL_MONGODB_URI".to_owned(),
                mongodb_connection_uri(
                    credential.username(),
                    credential.password(),
                    database_name,
                    credential.authentication_database(),
                ),
            ),
            (
                "STACKCTL_MONGODB_DATABASE".to_owned(),
                database_name.to_owned(),
            ),
        ]),
        None,
    )
    .map_err(|error| operation_error("v7 MongoDB backup request is invalid", error))
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
