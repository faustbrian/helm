use super::verify_v7_minio_source::{verification_request, verify_output};
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, OwnedContainer, run_attached_command_capture,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::shared_infrastructure::ObjectStoreProjectDefinition;
use crate::control_plane::state::CredentialRecord;
use std::time::Duration;

pub(super) async fn verify_v7_minio_target(
    executor: &(impl CommandExecutor + Sync),
    container: &OwnedContainer,
    definition: &ObjectStoreProjectDefinition,
    credential: &CredentialRecord,
    timeout: Duration,
) -> Result<(), MigrationOperationError> {
    let request = verification_request(
        credential.username(),
        credential.secret(),
        definition.bucket(),
    )?;
    let command = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "verify v8 MinIO migration target",
        timeout,
    )
    .map_err(|error| operation_error("v8 MinIO verification is invalid", error))?;
    let output = run_attached_command_capture(executor, container, &command)
        .await
        .map_err(|error| operation_error("verify v8 MinIO migration target", error))?;
    verify_output(definition.bucket(), &output)
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
