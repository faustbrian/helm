use super::ProjectVolumeRestoreOptions;
use crate::control_plane::engine::{
    ContainerHealth, ContainerLifecycle, ContainerState, ContainerVolumeArchive, HealthObserver,
    OwnedContainer, OwnedVolume, ResourceKind, RetentionClass, VolumeManager,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::retention::verify_resource_recovery_point_artifact;
use crate::control_plane::state::{ResourceLifecycle, ResourceRetention};

/// Recreates one exact empty volume and restores its verified archive before start.
pub(crate) async fn restore_project_volume<E>(
    engine: &mut E,
    container: &OwnedContainer,
    volume: &OwnedVolume,
    options: &ProjectVolumeRestoreOptions<'_>,
) -> Result<(), MigrationOperationError>
where
    E: ContainerLifecycle + ContainerVolumeArchive + HealthObserver + VolumeManager,
{
    validate(container, volume, options)?;
    let stored = verify_resource_recovery_point_artifact(
        options.recovery_point,
        options.resource,
        options.verified_at_unix_seconds,
    )
    .map_err(|error| operation_error("project volume recovery verification failed", error))?;
    let operation = async {
        match engine
            .inspect(container)
            .await
            .map_err(|error| operation_error("project volume container inspection failed", error))?
        {
            ContainerState::Running => engine
                .stop(container)
                .await
                .map_err(|error| operation_error("project volume service stop failed", error))?,
            ContainerState::Stopped => {}
            ContainerState::Missing => {
                return Err(MigrationOperationError::new(
                    "project volume restore container is missing",
                ));
            }
        }
        engine
            .remove(container)
            .await
            .map_err(|error| operation_error("project volume service removal failed", error))?;
        engine
            .remove_volume(volume)
            .await
            .map_err(|error| operation_error("project volume source removal failed", error))?;
        let restored_volume = engine
            .create_volume(options.desired_volume)
            .await
            .map_err(|error| operation_error("project volume recreation failed", error))?;
        if restored_volume.name() != options.desired_volume.name()
            || restored_volume.metadata() != options.desired_volume.metadata()
        {
            return Err(MigrationOperationError::new(
                "project volume recreation returned unexpected ownership",
            ));
        }
        let restored_container = engine
            .create(options.desired_container)
            .await
            .map_err(|error| operation_error("project volume service recreation failed", error))?;
        if restored_container.metadata() != options.desired_container.metadata() {
            return Err(MigrationOperationError::new(
                "project volume service recreation returned unexpected ownership",
            ));
        }
        engine
            .upload_volume_archive(
                &restored_container,
                &restored_volume,
                stored.artifact_file(),
            )
            .await
            .map_err(|error| operation_error("project volume archive restore failed", error))?;
        engine
            .start(&restored_container)
            .await
            .map_err(|error| operation_error("project volume service start failed", error))?;
        if engine
            .inspect(&restored_container)
            .await
            .map_err(|error| operation_error("restored project volume inspection failed", error))?
            != ContainerState::Running
        {
            return Err(MigrationOperationError::new(
                "restored project volume service did not remain running",
            ));
        }
        loop {
            let health = engine
                .observe_health(&restored_container)
                .await
                .map_err(|error| operation_error("restored project volume health failed", error))?;
            match health {
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
                        "restored project volume service did not become ready",
                    ));
                }
            }
        }

        Ok(())
    };

    tokio::time::timeout(options.timeout, operation)
        .await
        .map_err(|_| MigrationOperationError::new("project volume restore timed out"))?
}

fn validate(
    container: &OwnedContainer,
    volume: &OwnedVolume,
    options: &ProjectVolumeRestoreOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let resource = options.resource;
    let desired_container = options.desired_container.metadata();
    let desired_volume = options.desired_volume.metadata();
    let has_exact_mount = options
        .desired_container
        .volume_mounts()
        .iter()
        .filter(|mount| mount.source() == options.desired_volume.name())
        .count()
        == 1;
    let valid = !options.installation_id.is_empty()
        && !options.project_id.is_empty()
        && !options.service_id.is_empty()
        && options.verified_at_unix_seconds >= 0
        && !options.timeout.is_zero()
        && resource.resource_id() == volume.name()
        && resource.resource_id() == options.desired_volume.name()
        && resource.installation_id() == options.installation_id
        && resource.kind() == "volume"
        && resource.project_id() == Some(options.project_id)
        && resource.scope_id() == Some(options.service_id)
        && resource.retention() == ResourceRetention::Persistent
        && resource.lifecycle() == ResourceLifecycle::Active
        && volume.metadata() == desired_volume
        && desired_volume.kind() == ResourceKind::Volume
        && desired_volume.retention() == RetentionClass::Persistent
        && desired_container.kind() == ResourceKind::ProjectService
        && desired_container.retention() == RetentionClass::Disposable
        && desired_container.installation_id() == options.installation_id
        && desired_container.project_id() == Some(options.project_id)
        && desired_container.resource_id() == Some(options.service_id)
        && desired_container.compatibility_fingerprint() == resource.compatibility_fingerprint()
        && container.metadata() == desired_container
        && has_exact_mount;
    if !valid {
        return Err(MigrationOperationError::new(
            "project volume restore does not match exact active desired ownership",
        ));
    }

    Ok(())
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
