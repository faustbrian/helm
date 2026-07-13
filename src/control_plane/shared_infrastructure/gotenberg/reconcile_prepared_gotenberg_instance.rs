use super::PreparedGotenbergSharedInstance;
use crate::control_plane::engine::{
    ContainerDiscovery, ContainerLifecycle, HealthObserver, VolumeDiscovery, VolumeManager,
};
use crate::control_plane::shared_infrastructure::{
    SharedInfrastructureReconcileError, SharedInstanceReconcileResult,
    SharedServiceReconcileOptions, reconcile_shared_service,
};

/// Converges one stateless process and records every endpoint consumer.
pub(crate) async fn reconcile_prepared_gotenberg_instance<Engine>(
    engine: &mut Engine,
    prepared: &PreparedGotenbergSharedInstance,
    installation_id: &str,
    schema_version: u32,
) -> Result<SharedInstanceReconcileResult, SharedInfrastructureReconcileError>
where
    Engine:
        ContainerDiscovery + ContainerLifecycle + HealthObserver + VolumeDiscovery + VolumeManager,
{
    let shared = reconcile_shared_service(
        engine,
        SharedServiceReconcileOptions {
            request: prepared.instance().container(),
            volume: None,
            installation_id,
            schema_version,
        },
    )
    .await?;
    let logical = prepared
        .projects()
        .iter()
        .map(|project| prepared.logical_record(project, &shared))
        .collect();

    Ok(SharedInstanceReconcileResult::new(
        shared.container().id().as_str(),
        shared.container().metadata(),
        None,
        logical,
        shared.health(),
    ))
}
