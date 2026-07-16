use super::wait_for_postgres_readiness::wait_for_postgres_readiness;
use super::{PreparedPostgresSharedInstance, provision_postgres_logical_resource};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerLifecycle, HealthObserver, VolumeDiscovery,
    VolumeManager,
};
use crate::control_plane::shared_infrastructure::{
    SharedInfrastructureReconcileError, SharedInstanceReconcileResult,
    SharedServiceReconcileOptions, classify_logical_resource_error, reconcile_shared_service,
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
    wait_for_postgres_readiness(
        engine,
        shared.container(),
        prepared.instance().bootstrap_credential(),
    )
    .await
    .map_err(|error| SharedInfrastructureReconcileError::Engine {
        action: "PostgreSQL readiness".to_owned(),
        detail: error.to_string(),
    })?;
    let mut logical = Vec::with_capacity(prepared.projects().len());
    let mut logical_resource_drifts = Vec::new();

    for project in prepared.projects() {
        let result = provision_postgres_logical_resource(
            engine,
            shared.container(),
            project.logical(),
            prepared.instance().bootstrap_credential(),
        )
        .await
        .map_err(|error| {
            classify_logical_resource_error(
                project.logical().database_name(),
                "PostgreSQL logical resource provisioning",
                error,
            )
        });
        if let Err(error) = result {
            logical_resource_drifts.push(error.into_logical_resource_drift()?);

            continue;
        }
        logical.push(prepared.logical_record(project, &shared));
    }

    Ok(SharedInstanceReconcileResult::new(
        shared.container().clone(),
        shared
            .volume()
            .map(|volume| (volume.volume().name(), volume.volume().metadata())),
        logical,
        shared.health(),
    )
    .with_logical_resource_drifts(logical_resource_drifts))
}
