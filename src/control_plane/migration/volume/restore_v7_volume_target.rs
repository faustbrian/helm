use super::V7VolumeTargetRestoreOptions;
use crate::control_plane::engine::{
    ContainerHealth, ContainerLifecycle, ContainerState, ContainerVolumeArchive, HealthObserver,
    OwnedContainer, OwnedVolume, VolumeManager,
};
use crate::control_plane::migration::MigrationOperationError;

/// Recreates and verifies one exact prepared-v8 target from accepted-v7 data.
pub(crate) async fn restore_v7_volume_target<E>(
    engine: &mut E,
    container: &OwnedContainer,
    volume: &OwnedVolume,
    options: &V7VolumeTargetRestoreOptions<'_>,
) -> Result<(OwnedContainer, OwnedVolume), MigrationOperationError>
where
    E: ContainerLifecycle + ContainerVolumeArchive + HealthObserver + VolumeManager,
{
    validate(container, volume, options)?;
    let operation = async {
        match engine
            .inspect(container)
            .await
            .map_err(|error| operation_error("v8 volume target inspection failed", error))?
        {
            ContainerState::Running => engine
                .stop(container)
                .await
                .map_err(|error| operation_error("v8 volume target stop failed", error))?,
            ContainerState::Stopped => {}
            ContainerState::Missing => {
                return Err(MigrationOperationError::new(
                    "prepared v8 volume target container is missing",
                ));
            }
        }
        engine
            .remove(container)
            .await
            .map_err(|error| operation_error("v8 volume target removal failed", error))?;
        engine
            .remove_volume(volume)
            .await
            .map_err(|error| operation_error("v8 volume target reset failed", error))?;
        let restored_volume = engine
            .create_volume(options.desired_volume)
            .await
            .map_err(|error| operation_error("v8 volume target creation failed", error))?;
        if restored_volume.name() != options.desired_volume.name()
            || restored_volume.metadata() != options.desired_volume.metadata()
        {
            return Err(MigrationOperationError::new(
                "v8 volume target creation returned unexpected ownership",
            ));
        }
        let restored_container = engine
            .create(options.desired_container)
            .await
            .map_err(|error| operation_error("v8 volume container creation failed", error))?;
        if restored_container.metadata() != options.desired_container.metadata() {
            return Err(MigrationOperationError::new(
                "v8 volume container creation returned unexpected ownership",
            ));
        }
        engine
            .upload_volume_archive(&restored_container, &restored_volume, options.archive)
            .await
            .map_err(|error| operation_error("v8 volume archive restore failed", error))?;
        engine
            .start(&restored_container)
            .await
            .map_err(|error| operation_error("v8 volume target start failed", error))?;
        if engine
            .inspect(&restored_container)
            .await
            .map_err(|error| operation_error("restored v8 volume inspection failed", error))?
            != ContainerState::Running
        {
            return Err(MigrationOperationError::new(
                "restored v8 volume target did not remain running",
            ));
        }
        loop {
            match engine
                .observe_health(&restored_container)
                .await
                .map_err(|error| operation_error("restored v8 volume health failed", error))?
            {
                ContainerHealth::Healthy => break,
                ContainerHealth::RunningUnverified
                    if options.desired_container.health_check().is_none() =>
                {
                    break;
                }
                ContainerHealth::Starting | ContainerHealth::RunningUnverified => {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                ContainerHealth::Missing
                | ContainerHealth::Stopped
                | ContainerHealth::Unhealthy { .. } => {
                    return Err(MigrationOperationError::new(
                        "restored v8 volume target did not become ready",
                    ));
                }
            }
        }

        Ok((restored_container, restored_volume))
    };

    tokio::time::timeout(options.timeout, operation)
        .await
        .map_err(|_| MigrationOperationError::new("v8 volume target restore timed out"))?
}

fn validate(
    container: &OwnedContainer,
    volume: &OwnedVolume,
    options: &V7VolumeTargetRestoreOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let exact_mounts = options
        .desired_container
        .volume_mounts()
        .iter()
        .filter(|mount| {
            mount.source() == options.desired_volume.name()
                && mount.target() == options.expected_mount_target
        })
        .count();
    let valid = options.archive.is_absolute()
        && options.archive.is_file()
        && !options.timeout.is_zero()
        && exact_mounts == 1
        && options.desired_container.volume_mounts().len() == 1
        && container.metadata() == options.desired_container.metadata()
        && volume.name() == options.desired_volume.name()
        && volume.metadata() == options.desired_volume.metadata();
    if !valid {
        return Err(MigrationOperationError::new(
            "v8 volume restore does not match exact prepared ownership and mount mapping",
        ));
    }

    Ok(())
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
