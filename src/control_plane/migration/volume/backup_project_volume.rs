use super::ProjectVolumeBackupOptions;
use crate::control_plane::engine::{
    ContainerLifecycle, ContainerState, ContainerVolumeArchive, OwnedContainer, OwnedVolume,
};
use crate::control_plane::migration::{MigrationBackup, MigrationOperationError};
use crate::control_plane::retention::{
    BackupResourceIdentity, store_backup_artifact_from_async_reader, verify_stored_backup_artifact,
};
use crate::control_plane::state::{ResourceLifecycle, ResourceRetention};
use tokio::io::{AsyncWriteExt, duplex};

const STREAM_BUFFER_BYTES: usize = 64 * 1024;

/// Stops one dedicated service, streams its exact volume, then restores state.
pub(crate) async fn backup_project_volume<E>(
    engine: &mut E,
    container: &OwnedContainer,
    volume: &OwnedVolume,
    options: &ProjectVolumeBackupOptions<'_>,
) -> Result<MigrationBackup, MigrationOperationError>
where
    E: ContainerLifecycle + ContainerVolumeArchive,
{
    validate(container, volume, options)?;
    let was_running = match engine
        .inspect(container)
        .await
        .map_err(|error| operation_error("project volume state inspection failed", error))?
    {
        ContainerState::Running => {
            engine
                .stop(container)
                .await
                .map_err(|error| operation_error("project volume quiesce failed", error))?;
            true
        }
        ContainerState::Stopped => false,
        ContainerState::Missing => {
            return Err(MigrationOperationError::new(
                "project volume backup container is missing",
            ));
        }
    };
    let backup = stream_backup(engine, container, volume, options).await;
    let restart = if was_running {
        engine
            .start(container)
            .await
            .map_err(|error| operation_error("project volume service restart failed", error))
    } else {
        Ok(())
    };

    match (backup, restart) {
        (Ok(backup), Ok(())) => Ok(backup),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(restart)) => Err(MigrationOperationError::new(format!(
            "{error}; project volume service restart also failed: {restart}"
        ))),
    }
}

async fn stream_backup(
    engine: &impl ContainerVolumeArchive,
    container: &OwnedContainer,
    volume: &OwnedVolume,
    options: &ProjectVolumeBackupOptions<'_>,
) -> Result<MigrationBackup, MigrationOperationError> {
    let identity = BackupResourceIdentity::from_resource(options.resource);
    let (mut backup_reader, mut archive_output) = duplex(STREAM_BUFFER_BYTES);
    let download = async {
        let result = engine
            .download_volume_archive(container, volume, &mut archive_output)
            .await;
        let close = archive_output.shutdown().await;
        result.map_err(|error| operation_error("project volume archive failed", error))?;
        close.map_err(|error| operation_error("project volume archive close failed", error))
    };
    let store = async {
        store_backup_artifact_from_async_reader(
            &identity,
            &mut backup_reader,
            options.created_at_unix_seconds,
            options.backup_root,
        )
        .await
        .map_err(|error| operation_error("project volume backup storage failed", error))
    };
    let (_, stored) = tokio::time::timeout(
        options.timeout,
        futures_util::future::try_join(download, store),
    )
    .await
    .map_err(|_| MigrationOperationError::new("project volume backup timed out"))??;
    let evidence = verify_stored_backup_artifact(&stored, options.created_at_unix_seconds)
        .map_err(|error| operation_error("project volume backup verification failed", error))?;
    let reference = stored.recovery_point().to_str().ok_or_else(|| {
        MigrationOperationError::new("project volume recovery point is not valid Unicode")
    })?;

    MigrationBackup::new(
        reference,
        evidence.artifact_sha256(),
        evidence.artifact_size_bytes(),
    )
}

fn validate(
    container: &OwnedContainer,
    volume: &OwnedVolume,
    options: &ProjectVolumeBackupOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let resource = options.resource;
    let valid = !options.installation_id.is_empty()
        && !options.project_id.is_empty()
        && !options.service_id.is_empty()
        && options.created_at_unix_seconds >= 0
        && !options.timeout.is_zero()
        && options.backup_root.is_absolute()
        && resource.resource_id() == volume.name()
        && resource.installation_id() == options.installation_id
        && resource.kind() == "volume"
        && resource.project_id() == Some(options.project_id)
        && resource.scope_id() == Some(options.service_id)
        && resource.retention() == ResourceRetention::Persistent
        && resource.lifecycle() == ResourceLifecycle::Active
        && resource.compatibility_fingerprint() == volume.metadata().compatibility_fingerprint()
        && volume.metadata().installation_id() == options.installation_id
        && volume.metadata().project_id() == Some(options.project_id)
        && volume.metadata().resource_id() == Some(options.service_id)
        && container.metadata().installation_id() == options.installation_id
        && container.metadata().project_id() == Some(options.project_id)
        && container.metadata().resource_id() == Some(options.service_id)
        && container.metadata().compatibility_fingerprint() == resource.compatibility_fingerprint();
    if !valid {
        return Err(MigrationOperationError::new(
            "project volume backup does not match exact active persistent ownership",
        ));
    }

    Ok(())
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
