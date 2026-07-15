use super::wait_for_mongodb_readiness::wait_for_mongodb_readiness;
use super::{
    MongoDbMigrationPreparationOptions, MongoDbMigrationTargetReconcileResult,
    prepare_mongodb_migration_target,
};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerHealth, ContainerLifecycle, HealthObserver,
    VolumeDiscovery, VolumeManager,
};
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, CredentialSecret, SharedInfrastructureReconcileError, SharedInstancePlan,
    store_credential_secret,
};
use crate::control_plane::state::StateStore;
use crate::control_plane::workload::{
    ProjectVolumeReconcileOptions, WorkloadReconcileOptions, reconcile_project_volume,
    reconcile_retained_project_service,
};

/// Persists its secret and converges one isolated retained MongoDB target.
pub(crate) async fn reconcile_mongodb_migration_target<Store, Engine, Entropy>(
    store: &mut Store,
    engine: &mut Engine,
    shared: &SharedInstancePlan,
    entropy: &Entropy,
    options: MongoDbMigrationPreparationOptions<'_>,
) -> Result<MongoDbMigrationTargetReconcileResult, SharedInfrastructureReconcileError>
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
    let plan = prepare_mongodb_migration_target(store, shared, entropy, options)
        .map_err(|error| invalid("prepare MongoDB migration target", error))?;
    store_credential_secret(
        &CredentialSecret::new(plan.bootstrap_credential().secret().to_owned()),
        plan.bootstrap_secret_file(),
    )
    .map_err(|error| SharedInfrastructureReconcileError::InvalidRequest {
        detail: error.to_string(),
    })?;
    let request =
        plan.volume()
            .ok_or_else(|| SharedInfrastructureReconcileError::InvalidRequest {
                detail: "MongoDB migration target requires a retained data volume".to_owned(),
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
    .map_err(|error| invalid("reconcile MongoDB migration target volume", error))?;
    let service = reconcile_retained_project_service(
        engine,
        WorkloadReconcileOptions {
            request: plan.container(),
            installation_id: options.installation_id,
            schema_version: options.schema_version,
        },
    )
    .await
    .map_err(|error| invalid("reconcile MongoDB migration target service", error))?;
    wait_for_mongodb_readiness(engine, service.container(), plan.bootstrap_credential())
        .await
        .map_err(|error| invalid("wait for MongoDB migration target readiness", error))?;

    Ok(MongoDbMigrationTargetReconcileResult::new(
        plan,
        service,
        volume,
        ContainerHealth::Healthy,
    ))
}

fn invalid(action: &str, error: impl std::fmt::Display) -> SharedInfrastructureReconcileError {
    SharedInfrastructureReconcileError::Engine {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}
