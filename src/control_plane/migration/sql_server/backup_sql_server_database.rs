use super::SqlServerBackupOptions;
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

const SQLCMD_PATH: &str = "/opt/mssql-tools18/bin/sqlcmd";
const STREAM_BUFFER_BYTES: usize = 64 * 1024;
const BACKUP_SCRIPT: &str = "set -eu\n\
    trap 'rm -f \"$STACKCTL_BACKUP_FILE\"' EXIT HUP INT TERM\n\
    \"$STACKCTL_SQLCMD\" -b -C -S 127.0.0.1 -U \"$STACKCTL_SQLCMD_USER\" \
    -d \"$STACKCTL_DATABASE\" -Q \"$STACKCTL_BACKUP_SQL\" -o /dev/null\n\
    cat \"$STACKCTL_BACKUP_FILE\"";

/// Streams one native checksummed backup into an immutable recovery point.
pub(crate) async fn backup_sql_server_database(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &SqlServerBackupOptions<'_>,
) -> Result<MigrationBackup, MigrationOperationError> {
    validate(container, options)?;
    let backup_file = format!(
        "/var/opt/mssql/data/.stackctl-{}-{}.bak",
        options.database_name, options.created_at_unix_seconds
    );
    let backup_sql = format!(
        "BACKUP DATABASE [{}] TO DISK = N'{}' WITH COPY_ONLY, INIT, CHECKSUM",
        options.database_name, backup_file
    );
    let request = CommandRequest::new(
        vec!["sh".to_owned(), "-c".to_owned(), BACKUP_SCRIPT.to_owned()],
        BTreeMap::from([
            (
                "SQLCMDPASSWORD".to_owned(),
                options.credential.secret().to_owned(),
            ),
            ("STACKCTL_BACKUP_FILE".to_owned(), backup_file),
            ("STACKCTL_BACKUP_SQL".to_owned(), backup_sql),
            (
                "STACKCTL_DATABASE".to_owned(),
                options.database_name.to_owned(),
            ),
            ("STACKCTL_SQLCMD".to_owned(), SQLCMD_PATH.to_owned()),
            (
                "STACKCTL_SQLCMD_USER".to_owned(),
                options.credential.username().to_owned(),
            ),
        ]),
        None,
    )
    .map_err(|error| operation_error("SQL Server backup request is invalid", error))?;
    let command =
        StreamingCommandOptions::new(request, "back up SQL Server database", options.timeout)
            .map_err(|error| operation_error("SQL Server backup request is invalid", error))?;
    let identity =
        BackupResourceIdentity::from_logical(options.logical_resource, options.installation_id);
    let (mut backup_reader, mut command_output) = duplex(STREAM_BUFFER_BYTES);
    let mut command_input = tokio::io::empty();
    let backup = async {
        let result = run_streaming_command(
            executor,
            container,
            &command,
            &mut command_input,
            &mut command_output,
        )
        .await;
        let close = command_output.shutdown().await;
        result.map_err(|error| operation_error("SQL Server backup failed", error))?;
        close.map_err(|error| operation_error("SQL Server backup output close failed", error))
    };
    let store = async {
        store_backup_artifact_from_async_reader(
            &identity,
            &mut backup_reader,
            options.created_at_unix_seconds,
            options.backup_root,
        )
        .await
        .map_err(|error| operation_error("SQL Server backup storage failed", error))
    };
    let (_, stored) = futures_util::future::try_join(backup, store).await?;
    let evidence = verify_stored_backup_artifact(&stored, options.created_at_unix_seconds)
        .map_err(|error| operation_error("SQL Server backup verification failed", error))?;
    let reference = stored.recovery_point().to_str().ok_or_else(|| {
        MigrationOperationError::new("SQL Server backup recovery point is not valid Unicode")
    })?;

    MigrationBackup::new(
        reference,
        evidence.artifact_sha256(),
        evidence.artifact_size_bytes(),
    )
}

fn validate(
    container: &OwnedContainer,
    options: &SqlServerBackupOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let logical = options.logical_resource;
    let credential = options.credential;
    let invalid = !valid_identifier(options.database_name)
        || options.installation_id.is_empty()
        || options.created_at_unix_seconds < 0
        || !options.backup_root.is_absolute()
        || options.backup_root.to_str().is_none()
        || credential.project_id() != Some(logical.project_id())
        || credential.service_id() != logical.service_id()
        || !valid_identifier(credential.username())
        || credential.secret().is_empty()
        || options.timeout.is_zero()
        || logical.kind() != "sqlserver_database"
        || logical.logical_resource_id() != options.database_name
        || logical.lifecycle() != ResourceLifecycle::Active
        || credential.lifecycle() != CredentialLifecycle::Active
        || container.metadata().installation_id() != options.installation_id
        || container.metadata().compatibility_fingerprint() != logical.compatibility_fingerprint();
    if invalid {
        return Err(MigrationOperationError::new(
            "SQL Server backup request does not match an active owned logical resource",
        ));
    }

    Ok(())
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().enumerate().all(|(index, byte)| match byte {
            b'a'..=b'z' | b'_' => true,
            b'0'..=b'9' => index > 0,
            _ => false,
        })
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
