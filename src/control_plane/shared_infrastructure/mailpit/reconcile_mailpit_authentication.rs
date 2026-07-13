use super::{
    MailpitAuthenticationSnapshot, MailpitSharedInstancePlan, store_mailpit_authentication,
};
use crate::control_plane::engine::{
    ContainerDiscovery, ContainerLifecycle, HealthObserver, VolumeDiscovery, VolumeManager,
};
use crate::control_plane::shared_infrastructure::{
    SharedInfrastructureReconcileError, SharedServiceReconcileOptions,
    SharedServiceReconcileResult, reconcile_shared_service,
};
use std::path::Path;

/// Publishes attributed SMTP users before converging one shared Mailpit.
pub(crate) async fn reconcile_mailpit_authentication<E>(
    engine: &mut E,
    instance: &MailpitSharedInstancePlan,
    snapshot: &MailpitAuthenticationSnapshot,
    state_directory: &Path,
    installation_id: &str,
    schema_version: u32,
) -> Result<SharedServiceReconcileResult, SharedInfrastructureReconcileError>
where
    E: ContainerDiscovery + ContainerLifecycle + HealthObserver + VolumeDiscovery + VolumeManager,
{
    if instance.authentication_revision() != snapshot.revision() {
        return Err(SharedInfrastructureReconcileError::InvalidRequest {
            detail: format!(
                "Mailpit authentication revision '{}' does not match planned revision '{}'",
                snapshot.revision(),
                instance.authentication_revision()
            ),
        });
    }
    let expected_mount = state_directory.join("mounted");
    let expected_mount = expected_mount.to_str().ok_or_else(|| {
        SharedInfrastructureReconcileError::InvalidRequest {
            detail: format!(
                "Mailpit authentication state directory '{}' is not valid UTF-8",
                state_directory.display()
            ),
        }
    })?;
    let mounted = instance.container().bind_mounts().iter().any(|mount| {
        mount.source() == expected_mount
            && mount.target() == instance.authentication_mount_target()
            && mount.is_read_only()
    });
    if !mounted {
        return Err(SharedInfrastructureReconcileError::InvalidRequest {
            detail: format!(
                "Mailpit authentication mount must use managed directory '{expected_mount}'"
            ),
        });
    }

    store_mailpit_authentication(snapshot, state_directory).map_err(|error| {
        SharedInfrastructureReconcileError::Engine {
            action: "Mailpit authentication persistence".to_owned(),
            detail: error.to_string(),
        }
    })?;
    reconcile_shared_service(
        engine,
        SharedServiceReconcileOptions {
            request: instance.container(),
            volume: instance.volume(),
            installation_id,
            schema_version,
        },
    )
    .await
}
