use super::{
    PreparedRabbitMqSharedInstance, reconcile_rabbitmq_definitions, reconnect_rabbitmq_network,
};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerLifecycle, ContainerNetworkIsolation,
    HealthObserver, NetworkDiscovery, VolumeDiscovery, VolumeManager,
};
use crate::control_plane::shared_infrastructure::{
    SharedInfrastructureReconcileError, SharedInstanceReconcileResult,
};

/// Publishes complete broker definitions and converges the shared process once.
pub(crate) async fn reconcile_prepared_rabbitmq_instance<Engine>(
    engine: &mut Engine,
    prepared: &PreparedRabbitMqSharedInstance,
    installation_id: &str,
    schema_version: u32,
) -> Result<SharedInstanceReconcileResult, SharedInfrastructureReconcileError>
where
    Engine: CommandExecutor
        + ContainerDiscovery
        + ContainerLifecycle
        + ContainerNetworkIsolation
        + HealthObserver
        + NetworkDiscovery
        + VolumeDiscovery
        + VolumeManager,
{
    let shared = reconcile_rabbitmq_definitions(
        engine,
        prepared.instance(),
        prepared.definitions(),
        prepared.state_directory(),
        installation_id,
        schema_version,
    )
    .await?;
    reconnect_rabbitmq_network(
        engine,
        shared.container(),
        prepared.instance().container(),
        installation_id,
        schema_version,
    )
    .await?;
    let logical = prepared
        .projects()
        .iter()
        .map(|project| prepared.logical_record(project, &shared))
        .collect();

    Ok(SharedInstanceReconcileResult::new(
        shared.container().clone(),
        shared
            .volume()
            .map(|volume| (volume.volume().name(), volume.volume().metadata())),
        logical,
        shared.health(),
    ))
}
