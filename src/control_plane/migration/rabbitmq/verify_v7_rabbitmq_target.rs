use super::backup_v7_rabbitmq_vhost::{
    definitions_request, message_inventory_request, validate_no_messages,
};
use super::{V7RabbitMqCredential, transform_v7_rabbitmq_definitions};
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, OwnedContainer, run_attached_command_capture,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::shared_infrastructure::RabbitMqProjectDefinition;
use crate::control_plane::state::CredentialRecord;
use std::time::Duration;

pub(super) async fn verify_v7_rabbitmq_target(
    executor: &(impl CommandExecutor + Sync),
    container: &OwnedContainer,
    definition: &RabbitMqProjectDefinition,
    credential: &CredentialRecord,
    timeout: Duration,
) -> Result<(), MigrationOperationError> {
    let inventory = AttachedCommandOptions::new(
        message_inventory_request(definition.vhost())?,
        Vec::new(),
        "inventory v8 RabbitMQ migration target messages",
        timeout,
    )
    .map_err(|error| operation_error("v8 RabbitMQ message inventory is invalid", error))?;
    let output = run_attached_command_capture(executor, container, &inventory)
        .await
        .map_err(|error| operation_error("v8 RabbitMQ message inventory failed", error))?;
    validate_no_messages(definition.vhost(), &output)?;

    let export = AttachedCommandOptions::new(
        definitions_request(definition.vhost())?,
        Vec::new(),
        "export v8 RabbitMQ migration target definitions",
        timeout,
    )
    .map_err(|error| operation_error("v8 RabbitMQ definitions request is invalid", error))?;
    let definitions = run_attached_command_capture(executor, container, &export)
        .await
        .map_err(|error| operation_error("export v8 RabbitMQ definitions", error))?;
    let target_credential = V7RabbitMqCredential::new(credential.username(), credential.secret())
        .map_err(MigrationOperationError::new)?;
    transform_v7_rabbitmq_definitions(
        &definitions,
        definition.vhost(),
        &target_credential,
        definition,
    )?;
    Ok(())
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
