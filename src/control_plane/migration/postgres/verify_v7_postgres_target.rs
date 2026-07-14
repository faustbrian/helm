use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, OwnedContainer,
    run_attached_command_capture,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::shared_infrastructure::PostgresLogicalResourcePlan;
use crate::control_plane::state::CredentialRecord;
use std::collections::BTreeMap;
use std::time::Duration;

const TARGET_CATALOG_QUERY: &str = "SELECT current_database() || E'\\t' || \
pg_get_userbyid(datdba) || E'\\t' || \
(SELECT count(*)::text FROM pg_index WHERE NOT indisvalid) || E'\\t' || \
(SELECT count(*)::text FROM pg_constraint WHERE NOT convalidated) \
FROM pg_database WHERE datname = current_database();";

/// Verifies exact ownership and catalog validity in the restored v8 target.
pub(super) async fn verify_v7_postgres_target(
    executor: &(impl CommandExecutor + Sync),
    container: &OwnedContainer,
    plan: &PostgresLogicalResourcePlan,
    credential: &CredentialRecord,
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
            format!("--dbname={}", plan.database_name()),
            format!("--command={TARGET_CATALOG_QUERY}"),
        ],
        BTreeMap::from([("PGPASSWORD".to_owned(), credential.secret().to_owned())]),
        None,
    )
    .map_err(|error| operation_error("v8 PostgreSQL verification request is invalid", error))?;
    let command = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "verify v8 PostgreSQL migration target",
        timeout,
    )
    .map_err(|error| operation_error("v8 PostgreSQL verification request is invalid", error))?;
    let output = run_attached_command_capture(executor, container, &command)
        .await
        .map_err(|error| operation_error("verify v8 PostgreSQL migration target", error))?;
    let expected = format!("{}\t{}\t0\t0\n", plan.database_name(), plan.role_name());
    if output != expected.as_bytes() {
        return Err(MigrationOperationError::new(
            "v8 PostgreSQL target catalog returned unexpected evidence",
        ));
    }

    Ok(())
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
