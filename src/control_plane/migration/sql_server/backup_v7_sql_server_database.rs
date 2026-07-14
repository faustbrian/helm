use super::V7SqlServerCredential;
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

const SQLCMD_PATH: &str = "/opt/mssql-tools18/bin/sqlcmd";
const STREAM_BUFFER_BYTES: usize = 64 * 1024;
const BACKUP_SCRIPT: &str = "set -eu\ntrap 'rm -f \"$STACKCTL_BACKUP_FILE\"' EXIT HUP INT TERM\n\"$STACKCTL_SQLCMD\" -b -C -S 127.0.0.1 -U \"$STACKCTL_SQLCMD_USER\" -d master -Q \"$STACKCTL_BACKUP_SQL\" -o /dev/null\ncat \"$STACKCTL_BACKUP_FILE\"";

/// Streams one accepted native SQL Server backup into verified recovery.
pub(super) async fn backup_v7_sql_server_database(
    executor: &(impl V7ContainerCommandExecutor + Sync),
    target: &V7ContainerCommandTarget,
    credential: &V7SqlServerCredential,
    database_name: &str,
    identity: &BackupResourceIdentity,
    backup_root: &Path,
    created_at_unix_seconds: i64,
    verified_at_unix_seconds: i64,
    timeout: Duration,
) -> Result<MigrationBackup, MigrationOperationError> {
    let backup_file = format!("/var/opt/mssql/data/.stackctl-v7-{created_at_unix_seconds}.bak");
    let backup_sql = format!(
        "BACKUP DATABASE [{database_name}] TO DISK = N'{backup_file}' WITH COPY_ONLY, INIT, CHECKSUM"
    );
    let request = CommandRequest::new(
        vec!["sh".to_owned(), "-c".to_owned(), BACKUP_SCRIPT.to_owned()],
        BTreeMap::from([
            (
                "SQLCMDPASSWORD".to_owned(),
                credential.password().to_owned(),
            ),
            ("STACKCTL_BACKUP_FILE".to_owned(), backup_file),
            ("STACKCTL_BACKUP_SQL".to_owned(), backup_sql),
            ("STACKCTL_SQLCMD".to_owned(), SQLCMD_PATH.to_owned()),
            (
                "STACKCTL_SQLCMD_USER".to_owned(),
                credential.username().to_owned(),
            ),
        ]),
        None,
    )
    .map_err(|error| operation_error("v7 SQL Server backup request is invalid", error))?;
    let command =
        StreamingCommandOptions::new(request, "back up accepted v7 SQL Server database", timeout)
            .map_err(|error| operation_error("v7 SQL Server backup request is invalid", error))?;
    let (mut backup_reader, mut command_output) = duplex(STREAM_BUFFER_BYTES);
    let mut command_input = tokio::io::empty();
    let backup = async {
        let result = run_v7_streaming_command(
            executor,
            target,
            &command,
            &mut command_input,
            &mut command_output,
        )
        .await;
        let close = command_output.shutdown().await;
        result.map_err(|error| operation_error("v7 SQL Server backup failed", error))?;
        close.map_err(|error| operation_error("v7 SQL Server backup close failed", error))
    };
    let store = async {
        store_backup_artifact_from_async_reader(
            identity,
            &mut backup_reader,
            created_at_unix_seconds,
            backup_root,
        )
        .await
        .map_err(|error| operation_error("v7 SQL Server backup storage failed", error))
    };
    let (_, stored) = futures_util::future::try_join(backup, store).await?;
    let evidence = verify_stored_backup_artifact(&stored, verified_at_unix_seconds)
        .map_err(|error| operation_error("v7 SQL Server backup verification failed", error))?;
    let reference = stored.recovery_point().to_str().ok_or_else(|| {
        MigrationOperationError::new("v7 SQL Server recovery path is not valid Unicode")
    })?;
    MigrationBackup::new(
        reference,
        evidence.artifact_sha256(),
        evidence.artifact_size_bytes(),
    )
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
