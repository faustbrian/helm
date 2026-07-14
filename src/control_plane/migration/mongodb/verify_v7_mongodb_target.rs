use super::verify_v7_mongodb_source::verification_request;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, OwnedContainer, run_attached_command_capture,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::shared_infrastructure::MongoDbLogicalResourcePlan;
use crate::control_plane::state::CredentialRecord;
use std::time::Duration;

/// Verifies the deterministic v8 database and restricted-user identity.
pub(super) async fn verify_v7_mongodb_target(
    executor: &(impl CommandExecutor + Sync),
    container: &OwnedContainer,
    plan: &MongoDbLogicalResourcePlan,
    credential: &CredentialRecord,
    timeout: Duration,
) -> Result<(), MigrationOperationError> {
    let request = verification_request(
        credential.username(),
        credential.secret(),
        plan.database_name(),
        plan.database_name(),
    )?;
    let command = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "verify v8 MongoDB migration target",
        timeout,
    )
    .map_err(|error| operation_error("v8 MongoDB verification is invalid", error))?;
    let output = run_attached_command_capture(executor, container, &command)
        .await
        .map_err(|error| operation_error("verify v8 MongoDB migration target", error))?;
    let expected = format!("{}\n{}\n1\n", plan.database_name(), plan.username());
    if output != expected.as_bytes() {
        return Err(MigrationOperationError::new(
            "v8 MongoDB target returned unexpected identity evidence",
        ));
    }
    Ok(())
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
