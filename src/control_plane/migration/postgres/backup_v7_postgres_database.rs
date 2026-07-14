use super::V7PostgresCredential;
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

/// Streams one accepted v7 custom-format dump into private recovery storage.
pub(super) async fn backup_v7_postgres_database(
    executor: &(impl V7ContainerCommandExecutor + Sync),
    target: &V7ContainerCommandTarget,
    credential: &V7PostgresCredential,
    database_name: &str,
    identity: &BackupResourceIdentity,
    backup_root: &Path,
    created_at_unix_seconds: i64,
    verified_at_unix_seconds: i64,
    timeout: Duration,
) -> Result<MigrationBackup, MigrationOperationError> {
    let request = CommandRequest::new(
        vec![
            "pg_dump".to_owned(),
            "--format=custom".to_owned(),
            "--no-owner".to_owned(),
            "--no-privileges".to_owned(),
            format!("--username={}", credential.username()),
            format!("--dbname={database_name}"),
        ],
        BTreeMap::from([("PGPASSWORD".to_owned(), credential.password().to_owned())]),
        None,
    )
    .map_err(|error| operation_error("v7 PostgreSQL backup request is invalid", error))?;
    let command =
        StreamingCommandOptions::new(request, "dump accepted v7 PostgreSQL database", timeout)
            .map_err(|error| operation_error("v7 PostgreSQL backup request is invalid", error))?;
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
        result.map_err(|error| operation_error("v7 PostgreSQL backup failed", error))?;
        close.map_err(|error| operation_error("v7 PostgreSQL backup output close failed", error))
    };
    let store = async {
        store_backup_artifact_from_async_reader(
            identity,
            &mut backup_reader,
            created_at_unix_seconds,
            backup_root,
        )
        .await
        .map_err(|error| operation_error("store v7 PostgreSQL backup", error))
    };
    let (_, stored) = futures_util::future::try_join(dump, store).await?;
    let verified = verify_stored_backup_artifact(&stored, verified_at_unix_seconds)
        .map_err(|error| operation_error("verify v7 PostgreSQL backup", error))?;
    if !verified.matches_identity(identity) {
        return Err(MigrationOperationError::new(
            "v7 PostgreSQL backup identity differs from accepted evidence",
        ));
    }
    let reference = stored.recovery_point().to_str().ok_or_else(|| {
        MigrationOperationError::new("v7 PostgreSQL backup path is not valid UTF-8")
    })?;

    MigrationBackup::new(
        reference,
        verified.artifact_sha256(),
        verified.artifact_size_bytes(),
    )
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
