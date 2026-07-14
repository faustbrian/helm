use super::backup_v7_rabbitmq_vhost::{capture_v7_definitions, prove_v7_no_messages};
use super::{V7RabbitMqCredential, transform_v7_rabbitmq_definitions};
use crate::control_plane::engine::{V7ContainerCommandExecutor, V7ContainerCommandTarget};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::shared_infrastructure::RabbitMqProjectDefinition;
use std::time::Duration;

pub(super) async fn verify_v7_rabbitmq_identity(
    executor: &(impl V7ContainerCommandExecutor + Sync),
    target: &V7ContainerCommandTarget,
    source_vhost: &str,
    source_credential: &V7RabbitMqCredential,
    target_definition: &RabbitMqProjectDefinition,
    timeout: Duration,
) -> Result<(), MigrationOperationError> {
    prove_v7_no_messages(executor, target, source_vhost, timeout).await?;
    let definitions = capture_v7_definitions(executor, target, source_vhost, timeout).await?;
    transform_v7_rabbitmq_definitions(
        &definitions,
        source_vhost,
        source_credential,
        target_definition,
    )?;
    Ok(())
}
