use super::V7PostgresCredential;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandRequest, V7ContainerCommandExecutor, V7ContainerCommandTarget,
    run_v7_attached_command_capture,
};
use crate::control_plane::migration::MigrationOperationError;
use std::collections::BTreeMap;
use std::time::Duration;

const SOURCE_IDENTITY_QUERY: &str = "SELECT current_database() || E'\\t' || current_user;";

/// Proves that the exact retained v7 database remains reachable by its accepted user.
pub(super) async fn verify_v7_postgres_source(
    executor: &(impl V7ContainerCommandExecutor + Sync),
    target: &V7ContainerCommandTarget,
    credential: &V7PostgresCredential,
    database_name: &str,
    timeout: Duration,
) -> Result<(), MigrationOperationError> {
    let request = CommandRequest::new(
        vec![
            "psql".to_owned(),
            "--no-psqlrc".to_owned(),
            "--set=ON_ERROR_STOP=1".to_owned(),
            "--tuples-only".to_owned(),
            "--no-align".to_owned(),
            format!("--username={}", credential.username()),
            format!("--dbname={database_name}"),
            format!("--command={SOURCE_IDENTITY_QUERY}"),
        ],
        BTreeMap::from([("PGPASSWORD".to_owned(), credential.password().to_owned())]),
        None,
    )
    .map_err(|error| {
        operation_error(
            "v7 PostgreSQL source verification request is invalid",
            error,
        )
    })?;
    let command = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "verify retained v7 PostgreSQL source",
        timeout,
    )
    .map_err(|error| {
        operation_error(
            "v7 PostgreSQL source verification request is invalid",
            error,
        )
    })?;
    let output = run_v7_attached_command_capture(executor, target, &command)
        .await
        .map_err(|error| operation_error("verify retained v7 PostgreSQL source", error))?;
    let expected = format!("{database_name}\t{}\n", credential.username());
    if output != expected.as_bytes() {
        return Err(MigrationOperationError::new(
            "retained v7 PostgreSQL source returned unexpected identity evidence",
        ));
    }

    Ok(())
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
