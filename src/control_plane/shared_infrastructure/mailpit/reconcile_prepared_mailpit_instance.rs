use super::{PreparedMailpitSharedInstance, reconcile_mailpit_authentication};
use crate::control_plane::engine::{
    ContainerDiscovery, ContainerLifecycle, HealthObserver, VolumeDiscovery, VolumeManager,
};
use crate::control_plane::shared_infrastructure::{
    SharedInfrastructureReconcileError, SharedInstanceReconcileResult,
};

/// Publishes attributed SMTP authentication and converges one Mailpit process.
pub(crate) async fn reconcile_prepared_mailpit_instance<Engine>(
    engine: &mut Engine,
    prepared: &PreparedMailpitSharedInstance,
    installation_id: &str,
    schema_version: u32,
) -> Result<SharedInstanceReconcileResult, SharedInfrastructureReconcileError>
where
    Engine:
        ContainerDiscovery + ContainerLifecycle + HealthObserver + VolumeDiscovery + VolumeManager,
{
    let shared = reconcile_mailpit_authentication(
        engine,
        prepared.instance(),
        prepared.snapshot(),
        prepared.state_directory(),
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
        shared.container().id().as_str(),
        shared.container().metadata(),
        shared
            .volume()
            .map(|volume| (volume.volume().name(), volume.volume().metadata())),
        logical,
        shared.health(),
    ))
}
