use super::{
    PostgresProjectResources, PostgresSharedInstancePlan, provision_postgres_logical_resource,
};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerLifecycle, HealthObserver, VolumeDiscovery,
    VolumeManager,
};
use crate::control_plane::shared_infrastructure::{
    SharedInfrastructureReconcileError, SharedServiceReconcileOptions,
    SharedServiceReconcileResult, reconcile_shared_service,
};

/// Converges a shared PostgreSQL process and one isolated project tenant.
pub(crate) async fn reconcile_postgres_project_resources<E>(
    engine: &mut E,
    instance: &PostgresSharedInstancePlan,
    project: &PostgresProjectResources,
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
    provision_postgres_logical_resource(
        engine,
        shared.container(),
        project.logical(),
        instance.bootstrap_credential(),
    )
    .await
    .map_err(|error| SharedInfrastructureReconcileError::Engine {
        action: "PostgreSQL logical resource provisioning".to_owned(),
        detail: error.to_string(),
    })?;

    Ok(shared)
}
