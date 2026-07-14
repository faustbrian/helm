use super::V7SqlServerCredential;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandRequest, V7ContainerCommandExecutor, V7ContainerCommandTarget,
    run_v7_attached_command_capture,
};
use crate::control_plane::migration::MigrationOperationError;
use std::collections::BTreeMap;
use std::time::Duration;

const SQLCMD_PATH: &str = "/opt/mssql-tools18/bin/sqlcmd";
const VERIFY_SQL: &str = "SET NOCOUNT ON; SELECT DB_NAME() + CHAR(9) + SUSER_SNAME()";

pub(super) async fn verify_v7_sql_server_source(
    executor: &(impl V7ContainerCommandExecutor + Sync),
    target: &V7ContainerCommandTarget,
    credential: &V7SqlServerCredential,
    database_name: &str,
    timeout: Duration,
) -> Result<(), MigrationOperationError> {
    let request =
        verification_request(credential.username(), credential.password(), database_name)?;
    let command = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "verify retained v7 SQL Server source",
        timeout,
    )
    .map_err(|error| operation_error("v7 SQL Server verification is invalid", error))?;
    let output = run_v7_attached_command_capture(executor, target, &command)
        .await
        .map_err(|error| operation_error("verify retained v7 SQL Server source", error))?;
    let expected = format!("{database_name}\t{}\n", credential.username());
    if output != expected.as_bytes() {
        return Err(MigrationOperationError::new(
            "retained v7 SQL Server source returned unexpected identity evidence",
        ));
    }
    Ok(())
}

pub(super) fn verification_request(
    username: &str,
    password: &str,
    database_name: &str,
) -> Result<CommandRequest, MigrationOperationError> {
    CommandRequest::new(
        vec![
            SQLCMD_PATH.to_owned(),
            "-b".to_owned(),
            "-C".to_owned(),
            "-S".to_owned(),
            "127.0.0.1".to_owned(),
            "-U".to_owned(),
            username.to_owned(),
            "-d".to_owned(),
            database_name.to_owned(),
            "-h".to_owned(),
            "-1".to_owned(),
            "-W".to_owned(),
            "-Q".to_owned(),
            VERIFY_SQL.to_owned(),
        ],
        BTreeMap::from([("SQLCMDPASSWORD".to_owned(), password.to_owned())]),
        None,
    )
    .map_err(|error| operation_error("SQL Server verification request is invalid", error))
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
