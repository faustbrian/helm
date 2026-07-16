use super::wait_for_sql_server_readiness::wait_for_sql_server_readiness;
use super::{
    SqlServerMigrationPreparationOptions, SqlServerMigrationTargetReconcileResult,
    prepare_sql_server_migration_target,
};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerLifecycle, HealthObserver, VolumeDiscovery,
    VolumeManager,
};
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, SharedInfrastructureReconcileError, SharedInstancePlan,
};
use crate::control_plane::state::StateStore;
use crate::control_plane::workload::{
    ProjectVolumeReconcileOptions, WorkloadReconcileOptions, reconcile_project_volume,
    reconcile_retained_project_service,
};

/// Converges one isolated persistent SQL Server migration target.
pub(crate) async fn reconcile_sql_server_migration_target<Store, Engine, Entropy>(
    store: &mut Store,
    engine: &mut Engine,
    shared: &SharedInstancePlan,
    entropy: &Entropy,
    options: SqlServerMigrationPreparationOptions<'_>,
) -> Result<SqlServerMigrationTargetReconcileResult, SharedInfrastructureReconcileError>
where
    Store: StateStore,
    Engine: CommandExecutor
        + ContainerDiscovery
        + ContainerLifecycle
        + HealthObserver
        + VolumeDiscovery
        + VolumeManager,
    Entropy: CredentialEntropy,
{
    let plan = prepare_sql_server_migration_target(store, shared, entropy, options)
        .map_err(|error| invalid("prepare SQL Server migration target", error))?;
    let request =
        plan.volume()
            .ok_or_else(|| SharedInfrastructureReconcileError::InvalidRequest {
                detail: "SQL Server migration target requires a retained data volume".to_owned(),
            })?;
    let volume = reconcile_project_volume(
        engine,
        ProjectVolumeReconcileOptions {
            request,
            installation_id: options.installation_id,
            schema_version: options.schema_version,
        },
    )
    .await
    .map_err(|error| invalid("reconcile SQL Server migration target volume", error))?;
    let service = reconcile_retained_project_service(
        engine,
        WorkloadReconcileOptions {
            request: plan.container(),
            installation_id: options.installation_id,
            schema_version: options.schema_version,
        },
    )
    .await
    .map_err(|error| invalid("reconcile SQL Server migration target service", error))?;
    wait_for_sql_server_readiness(engine, service.container(), &plan)
        .await
        .map_err(|error| invalid("wait for SQL Server migration target readiness", error))?;

    Ok(SqlServerMigrationTargetReconcileResult::new(
        plan, service, volume,
    ))
}

fn invalid(action: &str, error: impl std::fmt::Display) -> SharedInfrastructureReconcileError {
    SharedInfrastructureReconcileError::Engine {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}
