use super::{
    SqlServerProjectResources, SqlServerSharedInstancePlan, provision_sql_server_logical_resource,
};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerLifecycle, HealthObserver, VolumeDiscovery,
    VolumeManager,
};
use crate::control_plane::shared_infrastructure::{
    SharedInfrastructureReconcileError, SharedServiceReconcileOptions,
    SharedServiceReconcileResult, reconcile_shared_service,
};

/// Converges one SQL Server process and isolated project database/login.
pub(crate) async fn reconcile_sql_server_project_resources<E>(
    engine: &mut E,
    instance: &SqlServerSharedInstancePlan,
    project: &SqlServerProjectResources,
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
    provision_sql_server_logical_resource(engine, shared.container(), instance, project.logical())
        .await
        .map_err(|error| SharedInfrastructureReconcileError::Engine {
            action: "SQL Server logical resource provisioning".to_owned(),
            detail: error.to_string(),
        })?;

    Ok(shared)
}
