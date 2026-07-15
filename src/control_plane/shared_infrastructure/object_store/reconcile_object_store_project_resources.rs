use super::{
    ObjectStoreFlavor, ObjectStoreProjectResources, ObjectStoreSharedInstancePlan,
    provision_object_store_project_resources, store_object_store_policy,
};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerLifecycle, HealthObserver, VolumeDiscovery,
    VolumeManager,
};
use crate::control_plane::shared_infrastructure::{
    SharedInfrastructureReconcileError, SharedServiceReconcileOptions,
    SharedServiceReconcileResult, classify_logical_resource_error, reconcile_shared_service,
};
use std::path::Path;

/// Persists policy state before converging MinIO and its project resources.
pub(crate) async fn reconcile_object_store_project_resources<E>(
    engine: &mut E,
    instance: &ObjectStoreSharedInstancePlan,
    project: &ObjectStoreProjectResources,
    policy_directory: &Path,
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
    if instance.flavor() != ObjectStoreFlavor::Minio {
        return Err(SharedInfrastructureReconcileError::InvalidRequest {
            detail: "RustFS IAM provisioning is not proven; refusing shared reconciliation"
                .to_owned(),
        });
    }
    let expected_source = policy_directory.to_str().ok_or_else(|| {
        SharedInfrastructureReconcileError::InvalidRequest {
            detail: format!(
                "object-store policy directory '{}' is not valid UTF-8",
                policy_directory.display()
            ),
        }
    })?;
    let mounted = instance.container().bind_mounts().iter().any(|mount| {
        mount.source() == expected_source
            && mount.target() == instance.policy_mount_target()
            && mount.is_read_only()
    });
    if !mounted {
        return Err(SharedInfrastructureReconcileError::InvalidRequest {
            detail: format!(
                "object-store policy mount must use managed directory '{expected_source}'"
            ),
        });
    }

    store_object_store_policy(project.definition(), policy_directory).map_err(|error| {
        SharedInfrastructureReconcileError::Engine {
            action: "object-store policy persistence".to_owned(),
            detail: error.to_string(),
        }
    })?;
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
    provision_object_store_project_resources(engine, shared.container(), instance, project)
        .await
        .map_err(|error| {
            classify_logical_resource_error(
                project.credential().credential_id(),
                "MinIO project-resource provisioning",
                error,
            )
        })?;

    Ok(shared)
}
