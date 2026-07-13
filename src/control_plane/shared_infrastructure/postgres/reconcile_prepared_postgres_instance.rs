use super::{PreparedPostgresSharedInstance, provision_postgres_logical_resource};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerLifecycle, HealthObserver, VolumeDiscovery,
    VolumeManager,
};
use crate::control_plane::shared_infrastructure::{
    SharedInfrastructureReconcileError, SharedInstanceReconcileResult,
    SharedServiceReconcileOptions, reconcile_shared_service,
};

/// Converges one physical PostgreSQL process and all isolated tenants once.
pub(crate) async fn reconcile_prepared_postgres_instance<Engine>(
    engine: &mut Engine,
    prepared: &PreparedPostgresSharedInstance,
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
    let shared = reconcile_shared_service(
        engine,
        SharedServiceReconcileOptions {
            request: prepared.instance().container(),
            volume: prepared.instance().volume(),
            installation_id,
            schema_version,
        },
    )
    .await?;
    let mut logical = Vec::with_capacity(prepared.projects().len());

    for project in prepared.projects() {
        provision_postgres_logical_resource(
            engine,
            shared.container(),
            project.logical(),
            prepared.instance().bootstrap_credential(),
        )
        .await
        .map_err(|error| SharedInfrastructureReconcileError::Engine {
            action: "PostgreSQL logical resource provisioning".to_owned(),
            detail: error.to_string(),
        })?;
        logical.push(prepared.logical_record(project, &shared));
    }

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
