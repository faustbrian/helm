use super::{
    RedisAclSnapshot, RedisSharedInstancePlan, reload_redis_acl, store_redis_acl_snapshot,
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

/// Atomically publishes and loads one complete Redis-compatible ACL snapshot.
pub(crate) async fn reconcile_redis_acl_snapshot<E>(
    engine: &mut E,
    instance: &RedisSharedInstancePlan,
    snapshot: &RedisAclSnapshot,
    state_directory: &Path,
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
    let expected_mount = state_directory.join("mounted");
    let expected_mount = expected_mount.to_str().ok_or_else(|| {
        SharedInfrastructureReconcileError::InvalidRequest {
            detail: format!(
                "Redis ACL state directory '{}' is not valid UTF-8",
                state_directory.display()
            ),
        }
    })?;
    let mounted = instance.container().bind_mounts().iter().any(|mount| {
        mount.source() == expected_mount
            && mount.target() == instance.acl_mount_target()
            && mount.is_read_only()
    });
    if !mounted {
        return Err(SharedInfrastructureReconcileError::InvalidRequest {
            detail: format!(
                "{} ACL mount must use managed snapshot directory '{expected_mount}'",
                instance.flavor().implementation()
            ),
        });
    }

    store_redis_acl_snapshot(snapshot, state_directory).map_err(|error| {
        SharedInfrastructureReconcileError::Engine {
            action: format!(
                "{} ACL snapshot persistence",
                instance.flavor().implementation()
            ),
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
    reload_redis_acl(engine, shared.container(), instance)
        .await
        .map_err(|error| {
            classify_logical_resource_error(
                shared.container().id().as_str(),
                format!("{} ACL reload", instance.flavor().implementation()),
                error,
            )
        })?;

    Ok(shared)
}
