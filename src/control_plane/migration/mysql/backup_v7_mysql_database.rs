use super::{V7MySqlCredential, reject_v7_mysql_explicit_definers};
use crate::control_plane::engine::{
    CommandRequest, StreamingCommandOptions, V7ContainerCommandExecutor, V7ContainerCommandTarget,
    run_v7_streaming_command,
};
use crate::control_plane::migration::{MigrationBackup, MigrationOperationError};
use crate::control_plane::retention::{
    BackupResourceIdentity, store_backup_artifact_from_async_reader, verify_stored_backup_artifact,
};
use crate::control_plane::shared_infrastructure::MySqlFlavor;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncWriteExt, duplex};

const STREAM_BUFFER_BYTES: usize = 64 * 1024;

/// Streams one accepted v7 schema-neutral dump into verified private recovery.
pub(super) async fn backup_v7_mysql_database(
    executor: &(impl V7ContainerCommandExecutor + Sync),
    target: &V7ContainerCommandTarget,
    flavor: MySqlFlavor,
    credential: &V7MySqlCredential,
    database_name: &str,
    identity: &BackupResourceIdentity,
    backup_root: &Path,
    created_at_unix_seconds: i64,
    verified_at_unix_seconds: i64,
    timeout: Duration,
) -> Result<MigrationBackup, MigrationOperationError> {
    let request = CommandRequest::new(
        dump_arguments(flavor, credential.username(), database_name),
        BTreeMap::from([("MYSQL_PWD".to_owned(), credential.password().to_owned())]),
        None,
    )
    .map_err(|error| operation_error("v7 MySQL-family backup request is invalid", error))?;
    let command =
        StreamingCommandOptions::new(request, "dump accepted v7 MySQL-family database", timeout)
            .map_err(|error| operation_error("v7 MySQL-family backup request is invalid", error))?;
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
        result.map_err(|error| operation_error("v7 MySQL-family backup failed", error))?;
        close.map_err(|error| operation_error("v7 MySQL-family backup close failed", error))
    };
    let store = async {
        store_backup_artifact_from_async_reader(
            identity,
            &mut backup_reader,
            created_at_unix_seconds,
            backup_root,
        )
        .await
        .map_err(|error| operation_error("v7 MySQL-family backup storage failed", error))
    };
    let (_, stored) = futures_util::future::try_join(dump, store).await?;
    if let Err(error) = reject_v7_mysql_explicit_definers(stored.artifact_file()).await {
        tokio::fs::remove_dir_all(stored.recovery_point())
            .await
            .map_err(|cleanup| {
                MigrationOperationError::new(format!(
                    "{error}; remove rejected v7 MySQL-family recovery: {cleanup}"
                ))
            })?;
        return Err(error);
    }
    let evidence = verify_stored_backup_artifact(&stored, verified_at_unix_seconds)
        .map_err(|error| operation_error("v7 MySQL-family backup verification failed", error))?;
    let reference = stored.recovery_point().to_str().ok_or_else(|| {
        MigrationOperationError::new("v7 MySQL-family recovery path is not valid Unicode")
    })?;

    MigrationBackup::new(
        reference,
        evidence.artifact_sha256(),
        evidence.artifact_size_bytes(),
    )
}

pub(super) fn dump_arguments(
    flavor: MySqlFlavor,
    username: &str,
    database_name: &str,
) -> Vec<String> {
    let mut arguments = vec![
        dump_executable(flavor).to_owned(),
        "--protocol=socket".to_owned(),
        format!("--user={username}"),
        "--single-transaction".to_owned(),
        "--quick".to_owned(),
        "--routines".to_owned(),
        "--events".to_owned(),
        "--triggers".to_owned(),
        "--hex-blob".to_owned(),
    ];
    if flavor == MySqlFlavor::MySql {
        arguments.push("--set-gtid-purged=OFF".to_owned());
    }
    arguments.push("--".to_owned());
    arguments.push(database_name.to_owned());

    arguments
}

const fn dump_executable(flavor: MySqlFlavor) -> &'static str {
    match flavor {
        MySqlFlavor::MySql => "mysqldump",
        MySqlFlavor::MariaDb => "mariadb-dump",
    }
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
