use super::V7MySqlCredential;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandRequest, V7ContainerCommandExecutor, V7ContainerCommandTarget,
    run_v7_attached_command_capture,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::shared_infrastructure::MySqlFlavor;
use std::collections::BTreeMap;
use std::time::Duration;

const IDENTITY_QUERY: &str =
    "SELECT CONCAT(DATABASE(), '\\t', SUBSTRING_INDEX(CURRENT_USER(), '@', 1));";

/// Proves the retained v7 schema remains reachable by its accepted user.
pub(super) async fn verify_v7_mysql_source(
    executor: &(impl V7ContainerCommandExecutor + Sync),
    target: &V7ContainerCommandTarget,
    flavor: MySqlFlavor,
    credential: &V7MySqlCredential,
    database_name: &str,
    timeout: Duration,
) -> Result<(), MigrationOperationError> {
    let request = verification_request(
        flavor,
        credential.username(),
        credential.password(),
        database_name,
    )?;
    let command = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "verify retained v7 MySQL-family source",
        timeout,
    )
    .map_err(|error| operation_error("v7 MySQL-family verification is invalid", error))?;
    let output = run_v7_attached_command_capture(executor, target, &command)
        .await
        .map_err(|error| operation_error("verify retained v7 MySQL-family source", error))?;
    let expected = format!("{database_name}\t{}\n", credential.username());
    if output != expected.as_bytes() {
        return Err(MigrationOperationError::new(
            "retained v7 MySQL-family source returned unexpected identity evidence",
        ));
    }

    Ok(())
}

pub(super) fn verification_request(
    flavor: MySqlFlavor,
    username: &str,
    password: &str,
    database_name: &str,
) -> Result<CommandRequest, MigrationOperationError> {
    CommandRequest::new(
        vec![
            client_executable(flavor).to_owned(),
            "--protocol=socket".to_owned(),
            format!("--user={username}"),
            format!("--database={database_name}"),
            "--batch".to_owned(),
            "--skip-column-names".to_owned(),
            format!("--execute={IDENTITY_QUERY}"),
        ],
        BTreeMap::from([("MYSQL_PWD".to_owned(), password.to_owned())]),
        None,
    )
    .map_err(|error| operation_error("MySQL-family verification request is invalid", error))
}

const fn client_executable(flavor: MySqlFlavor) -> &'static str {
    match flavor {
        MySqlFlavor::MySql => "mysql",
        MySqlFlavor::MariaDb => "mariadb",
    }
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
