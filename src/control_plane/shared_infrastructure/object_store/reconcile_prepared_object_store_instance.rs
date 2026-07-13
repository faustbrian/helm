use super::{
    ObjectStoreFlavor, PreparedObjectStoreSharedInstance, provision_object_store_project_resources,
    store_object_store_policy,
};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerLifecycle, HealthObserver, VolumeDiscovery,
    VolumeManager,
};
use crate::control_plane::shared_infrastructure::{
    SharedInfrastructureReconcileError, SharedInstanceReconcileResult,
    SharedServiceReconcileOptions, reconcile_shared_service,
};

/// Publishes all policies, converges one MinIO process, then provisions tenants.
pub(crate) async fn reconcile_prepared_object_store_instance<Engine>(
    engine: &mut Engine,
    prepared: &PreparedObjectStoreSharedInstance,
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
    if prepared.instance().flavor() != ObjectStoreFlavor::Minio {
        return Err(SharedInfrastructureReconcileError::InvalidRequest {
            detail: "only MinIO IAM provisioning is proven for shared object storage".to_owned(),
        });
    }
    for project in prepared.projects() {
        store_object_store_policy(project.definition(), prepared.policy_directory()).map_err(
            |error| SharedInfrastructureReconcileError::Engine {
                action: "object-store policy persistence".to_owned(),
                detail: error.to_string(),
            },
        )?;
    }
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
        provision_object_store_project_resources(
            engine,
            shared.container(),
            prepared.instance(),
            project,
        )
        .await
        .map_err(|error| SharedInfrastructureReconcileError::Engine {
            action: "MinIO project-resource provisioning".to_owned(),
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
    ))
}
