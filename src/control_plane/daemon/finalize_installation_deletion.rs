use crate::control_plane::application::ControlPlane;
use crate::control_plane::engine::{
    ContainerDiscovery, ContainerLifecycle, ImageDiscovery, ImageManager,
    InstallationResourceDeletionOptions, NetworkDiscovery, NetworkManager, VolumeDiscovery,
    VolumeManager, delete_owned_installation_resources,
};
use crate::control_plane::state::{InstallationLifecycle, StateStore};

/// Removes the exact Engine plane and commits terminal state when teardown is empty.
pub(crate) async fn finalize_installation_deletion<Store, Engine>(
    control_plane: &mut ControlPlane<Store>,
    engine: &mut Engine,
    schema_version: u32,
    now_unix_seconds: i64,
) -> Result<bool, String>
where
    Store: StateStore,
    Engine: ContainerDiscovery
        + ContainerLifecycle
        + ImageDiscovery
        + ImageManager
        + VolumeDiscovery
        + VolumeManager
        + NetworkDiscovery
        + NetworkManager,
{
    if control_plane
        .installation_lifecycle()
        .map_err(|error| error.to_string())?
        != Some(InstallationLifecycle::Deleting)
    {
        return Ok(false);
    }
    if !control_plane
        .logical_resources()
        .map_err(|error| error.to_string())?
        .is_empty()
        || !control_plane
            .active_daemon_operations()
            .map_err(|error| error.to_string())?
            .is_empty()
    {
        return Ok(false);
    }
    let installation = control_plane
        .installation()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "installation deletion identity is missing".to_owned())?;
    let installation_id = installation.installation_id().to_owned();
    let authorized_persistent_volumes =
        control_plane.verified_installation_volume_deletions(now_unix_seconds)?;
    delete_owned_installation_resources(
        engine,
        InstallationResourceDeletionOptions {
            installation_id: &installation_id,
            schema_version,
            authorized_persistent_volumes: &authorized_persistent_volumes,
        },
    )
    .await
    .map_err(|error| error.to_string())?;
    control_plane.complete_installation_deletion()?;

    Ok(true)
}
