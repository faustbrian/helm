use super::wait_for_postgres_readiness::wait_for_postgres_readiness;
use super::{
    PostgresMigrationPreparationOptions, PostgresMigrationTargetReconcileError,
    PostgresMigrationTargetReconcileResult, prepare_postgres_migration_target,
};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerHealth, ContainerLifecycle, HealthObserver,
    VolumeDiscovery, VolumeManager,
};
use crate::control_plane::shared_infrastructure::{CredentialEntropy, SharedInstancePlan};
use crate::control_plane::state::StateStore;
use crate::control_plane::workload::{
    ProjectVolumeReconcileOptions, WorkloadReconcileOptions, reconcile_project_volume,
    reconcile_retained_project_service,
};

/// Prepares and converges one isolated, retained PostgreSQL migration target.
pub(crate) async fn reconcile_postgres_migration_target<Store, Engine, Entropy>(
    store: &mut Store,
    engine: &mut Engine,
    shared: &SharedInstancePlan,
    entropy: &Entropy,
    options: PostgresMigrationPreparationOptions<'_>,
) -> Result<PostgresMigrationTargetReconcileResult, PostgresMigrationTargetReconcileError>
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
    let plan = prepare_postgres_migration_target(store, shared, entropy, options)
        .map_err(|error| invalid("prepare PostgreSQL migration target", error))?;
    let request = plan.volume().ok_or_else(|| {
        PostgresMigrationTargetReconcileError::new(
            "PostgreSQL migration target requires a retained data volume",
        )
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
    .map_err(|error| invalid("reconcile PostgreSQL migration target volume", error))?;
    let service = reconcile_retained_project_service(
        engine,
        WorkloadReconcileOptions {
            request: plan.container(),
            installation_id: options.installation_id,
            schema_version: options.schema_version,
        },
    )
    .await
    .map_err(|error| invalid("reconcile PostgreSQL migration target service", error))?;
    wait_for_postgres_readiness(engine, service.container(), plan.bootstrap_credential())
        .await
        .map_err(|error| invalid("verify PostgreSQL migration target readiness", error))?;

    Ok(PostgresMigrationTargetReconcileResult::new(
        plan,
        service,
        volume,
        ContainerHealth::Healthy,
    ))
}

fn invalid(action: &str, error: impl std::fmt::Display) -> PostgresMigrationTargetReconcileError {
    PostgresMigrationTargetReconcileError::new(format!("could not {action}: {error}"))
}
