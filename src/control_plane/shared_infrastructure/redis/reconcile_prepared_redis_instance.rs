use super::{PreparedRedisSharedInstance, reconcile_redis_acl_snapshot};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerLifecycle, HealthObserver, VolumeDiscovery,
    VolumeManager,
};
use crate::control_plane::shared_infrastructure::{
    SharedInfrastructureReconcileError, SharedInstanceReconcileResult,
};

/// Publishes one complete ACL snapshot and converges its shared process once.
pub(crate) async fn reconcile_prepared_redis_instance<Engine>(
    engine: &mut Engine,
    prepared: &PreparedRedisSharedInstance,
    installation_id: &str,
    schema_version: u32,
) -> Result<SharedInstanceReconcileResult, SharedInfrastructureReconcileError>
where
    Engine: CommandExecutor
        + ContainerDiscovery
        + ContainerLifecycle
        + HealthObserver
        + VolumeDiscovery
        + VolumeManager,
{
    let shared = reconcile_redis_acl_snapshot(
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
