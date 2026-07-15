use super::wait_for_mongodb_readiness::wait_for_mongodb_readiness;
use super::{PreparedMongoDbSharedInstance, provision_mongodb_logical_resource};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerLifecycle, HealthObserver, VolumeDiscovery,
    VolumeManager,
};
use crate::control_plane::shared_infrastructure::{
    CredentialSecret, SharedInfrastructureReconcileError, SharedInstanceReconcileResult,
    SharedServiceReconcileOptions, classify_logical_resource_error, reconcile_shared_service,
    store_credential_secret,
};

/// Stores the bootstrap secret, then converges one process and all tenants.
pub(crate) async fn reconcile_prepared_mongodb_instance<Engine>(
    engine: &mut Engine,
    prepared: &PreparedMongoDbSharedInstance,
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
    let bootstrap = CredentialSecret::new(
        prepared
            .instance()
            .bootstrap_credential()
            .secret()
            .to_owned(),
    );
    store_credential_secret(&bootstrap, prepared.bootstrap_secret_file()).map_err(|error| {
        SharedInfrastructureReconcileError::InvalidRequest {
            detail: error.to_string(),
        }
    })?;
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
    wait_for_mongodb_readiness(
        engine,
        shared.container(),
        prepared.instance().bootstrap_credential(),
    )
    .await
    .map_err(|error| SharedInfrastructureReconcileError::Engine {
        action: "MongoDB readiness".to_owned(),
        detail: error.to_string(),
    })?;
    let mut logical = Vec::with_capacity(prepared.projects().len());
    let mut logical_resource_drifts = Vec::new();

    for project in prepared.projects() {
        let result =
            provision_mongodb_logical_resource(engine, shared.container(), project.logical())
                .await
                .map_err(|error| {
                    classify_logical_resource_error(
                        project.credential().credential_id(),
                        "MongoDB logical resource provisioning",
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
        shared.container().id().as_str(),
        shared.container().metadata(),
        shared
            .volume()
            .map(|volume| (volume.volume().name(), volume.volume().metadata())),
        logical,
        shared.health(),
    )
    .with_logical_resource_drifts(logical_resource_drifts))
}
