use super::SqlServerVerifyTargetOptions;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, ResourceKind, RetentionClass,
    run_attached_command_capture,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::state::{CredentialLifecycle, MigrationPhase};
use std::collections::BTreeMap;

const SQLCMD_PATH: &str = "/opt/mssql-tools18/bin/sqlcmd";
const VERIFY_SQL: &str = "SET NOCOUNT ON; SELECT DB_NAME() + CHAR(9) + SUSER_SNAME()";

/// Proves the restored database through its project login, not SA.
pub(crate) async fn verify_sql_server_target(
    executor: &impl CommandExecutor,
    container: &crate::control_plane::engine::OwnedContainer,
    options: &SqlServerVerifyTargetOptions<'_>,
) -> Result<(), MigrationOperationError> {
    validate(container, options)?;
    let request = CommandRequest::new(
        vec![
            SQLCMD_PATH.to_owned(),
            "-b".to_owned(),
            "-C".to_owned(),
            "-S".to_owned(),
            "127.0.0.1".to_owned(),
            "-U".to_owned(),
            options.credential.username().to_owned(),
            "-d".to_owned(),
            options.target_database_name.to_owned(),
            "-h".to_owned(),
            "-1".to_owned(),
            "-W".to_owned(),
            "-Q".to_owned(),
            VERIFY_SQL.to_owned(),
        ],
        BTreeMap::from([(
            "SQLCMDPASSWORD".to_owned(),
            options.credential.secret().to_owned(),
        )]),
        None,
    )
    .map_err(|error| operation_error("SQL Server verification request is invalid", error))?;
    let command = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "verify restored SQL Server tenant",
        options.timeout,
    )
    .map_err(|error| operation_error("SQL Server verification request is invalid", error))?;
    let output = run_attached_command_capture(executor, container, &command)
        .await
        .map_err(|error| operation_error("SQL Server target verification failed", error))?;
    let expected = format!(
        "{}\t{}\n",
        options.target_database_name,
        options.credential.username()
    );
    if output != expected.as_bytes() {
        return Err(MigrationOperationError::new(
            "SQL Server target verification did not return the exact database and login",
        ));
    }

    Ok(())
}

fn validate(
    container: &crate::control_plane::engine::OwnedContainer,
    options: &SqlServerVerifyTargetOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let checkpoint = options.checkpoint;
    let metadata = container.metadata();
    let invalid = checkpoint.phase() != MigrationPhase::DataRestored
        || options.installation_id.is_empty()
        || options.target_database_name.is_empty()
        || checkpoint.target_resource_id() != Some(options.target_database_name)
        || options.credential.project_id() != Some(checkpoint.project_id())
        || options.credential.username().is_empty()
        || options.credential.secret().is_empty()
        || options.credential.lifecycle() != CredentialLifecycle::Active
        || options.timeout.is_zero()
        || metadata.installation_id() != options.installation_id
        || metadata.kind() != ResourceKind::ProjectService
        || metadata.project_id() != Some(checkpoint.project_id())
        || metadata.resource_id() != Some(checkpoint.migration_id())
        || metadata.retention() != RetentionClass::Persistent
        || metadata.compatibility_fingerprint() != checkpoint.target_compatibility_fingerprint();
    if invalid {
        return Err(MigrationOperationError::new(
            "SQL Server target verification does not match its restored target",
        ));
    }
    Ok(())
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
