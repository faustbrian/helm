use super::SqlServerRestoreOptions;
use crate::control_plane::engine::{
    CommandExecutor, CommandRequest, ResourceKind, RetentionClass, StreamingCommandOptions,
    run_streaming_command,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::retention::{
    BackupResourceIdentity, open_stored_backup_artifact, verify_stored_backup_artifact,
};
use crate::control_plane::state::{CredentialLifecycle, MigrationPhase, ResourceLifecycle};
use std::collections::BTreeMap;

const SQLCMD_PATH: &str = "/opt/mssql-tools18/bin/sqlcmd";
const RESTORE_SCRIPT: &str = "set -eu\n\
    trap 'rm -f \"$STACKCTL_RESTORE_FILE\"' EXIT HUP INT TERM\n\
    cat > \"$STACKCTL_RESTORE_FILE\"\n\
    \"$STACKCTL_SQLCMD\" -b -C -S 127.0.0.1 -U sa -d master \
    -Q \"$STACKCTL_VERIFY_SQL\" -o /dev/null\n\
    \"$STACKCTL_SQLCMD\" -b -C -S 127.0.0.1 -U sa -d master \
    -Q \"$STACKCTL_RESTORE_SQL\" -o /dev/null";

/// Restores one exact native backup into an isolated SQL Server target.
pub(crate) async fn restore_sql_server_database(
    executor: &impl CommandExecutor,
    container: &crate::control_plane::engine::OwnedContainer,
    options: &SqlServerRestoreOptions<'_>,
) -> Result<(), MigrationOperationError> {
    validate(container, options)?;
    let checkpoint = options.checkpoint;
    let reference = required(
        checkpoint.backup_reference(),
        "SQL Server restore checkpoint has no backup reference",
    )?;
    let expected_checksum = required(
        checkpoint.backup_artifact_sha256(),
        "SQL Server restore checkpoint has no backup checksum",
    )?;
    let expected_size = checkpoint.backup_artifact_size_bytes().ok_or_else(|| {
        MigrationOperationError::new("SQL Server restore checkpoint has no backup size")
    })?;
    let identity = BackupResourceIdentity::from_logical(
        options.source_logical_resource,
        options.installation_id,
    );
    let stored = open_stored_backup_artifact(reference)
        .map_err(|error| operation_error("SQL Server restore backup is unavailable", error))?;
    let evidence = verify_stored_backup_artifact(&stored, options.verified_at_unix_seconds)
        .map_err(|error| operation_error("SQL Server restore backup verification failed", error))?;
    if !evidence.matches_identity(&identity)
        || evidence.artifact_sha256() != expected_checksum
        || evidence.artifact_size_bytes() != expected_size
    {
        return Err(MigrationOperationError::new(
            "SQL Server restore backup does not match its durable checkpoint",
        ));
    }

    let restore_file = format!(
        "/var/opt/mssql/data/.stackctl-restore-{}.bak",
        checkpoint.migration_id()
    );
    let verify_sql = format!("RESTORE VERIFYONLY FROM DISK = N'{restore_file}' WITH CHECKSUM");
    let restore_sql = format!(
        "RESTORE DATABASE [{database}] FROM DISK = N'{restore_file}' \
         WITH REPLACE, RECOVERY, CHECKSUM; \
         USE [{database}]; \
         IF USER_ID(N'{username}') IS NOT NULL \
         ALTER USER [{username}] WITH LOGIN = [{username}];",
        database = options.target_database_name,
        username = options.credential.username(),
    );
    let request = CommandRequest::new(
        vec!["sh".to_owned(), "-c".to_owned(), RESTORE_SCRIPT.to_owned()],
        BTreeMap::from([
            (
                "SQLCMDPASSWORD".to_owned(),
                options.administrator.secret().to_owned(),
            ),
            ("STACKCTL_RESTORE_FILE".to_owned(), restore_file),
            ("STACKCTL_RESTORE_SQL".to_owned(), restore_sql),
            ("STACKCTL_SQLCMD".to_owned(), SQLCMD_PATH.to_owned()),
            ("STACKCTL_VERIFY_SQL".to_owned(), verify_sql),
        ]),
        None,
    )
    .map_err(|error| operation_error("SQL Server restore request is invalid", error))?;
    let command =
        StreamingCommandOptions::new(request, "restore SQL Server database", options.timeout)
            .map_err(|error| operation_error("SQL Server restore request is invalid", error))?;
    let mut artifact = tokio::fs::File::open(stored.artifact_file())
        .await
        .map_err(|error| operation_error("SQL Server restore artifact open failed", error))?;
    let mut output = tokio::io::sink();

    run_streaming_command(executor, container, &command, &mut artifact, &mut output)
        .await
        .map_err(|error| operation_error("SQL Server restore failed", error))
}

fn validate(
    container: &crate::control_plane::engine::OwnedContainer,
    options: &SqlServerRestoreOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let checkpoint = options.checkpoint;
    let expected_credential_id = format!(
        "migration/{}/sqlserver-bootstrap",
        checkpoint.migration_id()
    );
    let metadata = container.metadata();
    let invalid = checkpoint.phase() != MigrationPhase::TargetProvisioned
        || options.installation_id.is_empty()
        || !valid_identifier(options.target_database_name)
        || !safe_path_component(checkpoint.migration_id())
        || checkpoint.target_resource_id() != Some(options.target_database_name)
        || options.source_logical_resource.kind() != "sqlserver_database"
        || options.source_logical_resource.project_id() != checkpoint.project_id()
        || options.source_logical_resource.lifecycle() != ResourceLifecycle::Active
        || options.source_logical_resource.compatibility_fingerprint()
            != checkpoint.source_compatibility_fingerprint()
        || options.credential.project_id() != Some(checkpoint.project_id())
        || options.credential.service_id() != options.source_logical_resource.service_id()
        || !valid_identifier(options.credential.username())
        || options.credential.secret().is_empty()
        || options.credential.lifecycle() != CredentialLifecycle::Active
        || options.administrator.credential_id() != expected_credential_id
        || options.administrator.project_id() != Some(checkpoint.project_id())
        || options.administrator.service_id() != "sqlserver"
        || options.administrator.username() != "sa"
        || options.administrator.secret().is_empty()
        || options.administrator.lifecycle() != CredentialLifecycle::Active
        || options.timeout.is_zero()
        || options.verified_at_unix_seconds < checkpoint.updated_at_unix_seconds()
        || metadata.installation_id() != options.installation_id
        || metadata.kind() != ResourceKind::ProjectService
        || metadata.project_id() != Some(checkpoint.project_id())
        || metadata.resource_id() != Some(checkpoint.migration_id())
        || metadata.retention() != RetentionClass::Persistent
        || metadata.compatibility_fingerprint() != checkpoint.target_compatibility_fingerprint();
    if invalid {
        return Err(MigrationOperationError::new(
            "SQL Server restore request does not match its owned migration target",
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

fn safe_path_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && value
            .bytes()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-'))
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
