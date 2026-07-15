use super::{
    MongoDbProjectResources, MongoDbSharedInstancePlan, provision_mongodb_logical_resource,
};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerLifecycle, HealthObserver, VolumeDiscovery,
    VolumeManager,
};
use crate::control_plane::shared_infrastructure::{
    SharedInfrastructureReconcileError, SharedServiceReconcileOptions,
    SharedServiceReconcileResult, classify_logical_resource_error, reconcile_shared_service,
};

/// Converges a shared MongoDB process and one database-scoped project user.
pub(crate) async fn reconcile_mongodb_project_resources<E>(
    engine: &mut E,
    instance: &MongoDbSharedInstancePlan,
    project: &MongoDbProjectResources,
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
    provision_mongodb_logical_resource(engine, shared.container(), project.logical())
        .await
        .map_err(|error| {
            classify_logical_resource_error(
                project.credential().credential_id(),
                "MongoDB logical resource provisioning",
                error,
            )
        })?;

    Ok(shared)
}
