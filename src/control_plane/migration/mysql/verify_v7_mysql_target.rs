use super::verify_v7_mysql_source::verification_request;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, OwnedContainer, run_attached_command_capture,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::shared_infrastructure::{MySqlFlavor, MySqlLogicalResourcePlan};
use crate::control_plane::state::CredentialRecord;
use std::time::Duration;

/// Verifies the deterministic v8 schema and restricted-user identity.
pub(super) async fn verify_v7_mysql_target(
    executor: &(impl CommandExecutor + Sync),
    container: &OwnedContainer,
    flavor: MySqlFlavor,
    plan: &MySqlLogicalResourcePlan,
    credential: &CredentialRecord,
    timeout: Duration,
) -> Result<(), MigrationOperationError> {
    let request = verification_request(
        flavor,
        credential.username(),
        credential.secret(),
        plan.schema_name(),
    )?;
    let command = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "verify v8 MySQL-family migration target",
        timeout,
    )
    .map_err(|error| operation_error("v8 MySQL-family verification is invalid", error))?;
    let output = run_attached_command_capture(executor, container, &command)
        .await
        .map_err(|error| operation_error("verify v8 MySQL-family migration target", error))?;
    let expected = format!("{}\t{}\n", plan.schema_name(), plan.username());
    if output != expected.as_bytes() {
        return Err(MigrationOperationError::new(
            "v8 MySQL-family target returned unexpected identity evidence",
        ));
    }

    Ok(())
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
