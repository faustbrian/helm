use super::{
    RabbitMqDefinitions, RabbitMqSharedInstancePlan, reload_rabbitmq_definitions,
    store_rabbitmq_definitions,
};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerLifecycle, HealthObserver, VolumeDiscovery,
    VolumeManager,
};
use crate::control_plane::shared_infrastructure::{
    SharedInfrastructureReconcileError, SharedServiceReconcileOptions,
    SharedServiceReconcileResult, reconcile_shared_service,
};
use std::path::Path;

/// Atomically publishes and loads one complete RabbitMQ core-definitions set.
pub(crate) async fn reconcile_rabbitmq_definitions<E>(
    engine: &mut E,
    instance: &RabbitMqSharedInstancePlan,
    definitions: &RabbitMqDefinitions,
    state_directory: &Path,
    installation_id: &str,
    schema_version: u32,
) -> Result<SharedServiceReconcileResult, SharedInfrastructureReconcileError>
where
    E: CommandExecutor
        + ContainerDiscovery
        + ContainerLifecycle
        + HealthObserver
        + VolumeDiscovery
        + VolumeManager,
{
    let expected_mount = state_directory.join("mounted");
    let expected_mount = expected_mount.to_str().ok_or_else(|| {
        SharedInfrastructureReconcileError::InvalidRequest {
            detail: format!(
                "RabbitMQ definitions state directory '{}' is not valid UTF-8",
                state_directory.display()
            ),
        }
    })?;
    let mounted = instance.container().bind_mounts().iter().any(|mount| {
        mount.source() == expected_mount
            && mount.target() == instance.config_mount_target()
            && mount.is_read_only()
    });
    if !mounted {
        return Err(SharedInfrastructureReconcileError::InvalidRequest {
            detail: format!(
                "RabbitMQ definitions mount must use managed directory '{expected_mount}'"
            ),
        });
    }

    store_rabbitmq_definitions(definitions, state_directory).map_err(|error| {
        SharedInfrastructureReconcileError::Engine {
            action: "RabbitMQ definitions persistence".to_owned(),
            detail: error.to_string(),
        }
    })?;
    let shared = reconcile_shared_service(
        engine,
        SharedServiceReconcileOptions {
            request: instance.container(),
            volume: instance.volume(),
            installation_id,
            schema_version,
        },
    )
    .await?;
    reload_rabbitmq_definitions(engine, shared.container(), instance)
        .await
        .map_err(|error| SharedInfrastructureReconcileError::Engine {
            action: "RabbitMQ definitions reload".to_owned(),
            detail: error.to_string(),
        })?;

    Ok(shared)
}
