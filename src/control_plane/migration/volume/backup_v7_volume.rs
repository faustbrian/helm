use super::V7VolumeBackupOptions;
use crate::control_plane::engine::{
    ContainerState, V7ContainerCommandTarget, V7ContainerVolumeArchive, VolumeMount,
};
use crate::control_plane::migration::{MigrationBackup, MigrationOperationError};
use crate::control_plane::retention::{
    store_backup_artifact_from_async_reader, verify_stored_backup_artifact,
};
use tokio::io::{AsyncWriteExt, duplex};

const STREAM_BUFFER_BYTES: usize = 64 * 1024;

/// Quiesces one accepted-v7 container and stores its exact volume archive.
pub(crate) async fn backup_v7_volume(
    engine: &impl V7ContainerVolumeArchive,
    target: &V7ContainerCommandTarget,
    mounts: &[VolumeMount],
    options: &V7VolumeBackupOptions<'_>,
) -> Result<MigrationBackup, MigrationOperationError> {
    validate(mounts, options)?;
    let was_running = match engine
        .inspect_v7_volume_container(target, mounts)
        .await
        .map_err(|error| operation_error("v7 volume state inspection failed", error))?
    {
        ContainerState::Running => {
            engine
                .stop_v7_volume_container(target, mounts)
                .await
                .map_err(|error| operation_error("v7 volume quiesce failed", error))?;
            true
        }
        ContainerState::Stopped => false,
        ContainerState::Missing => {
            return Err(MigrationOperationError::new(
                "accepted v7 volume container is missing",
            ));
        }
    };
    let backup = stream_backup(engine, target, mounts, options).await;
    let restart = if was_running {
        engine
            .start_v7_volume_container(target, mounts)
            .await
            .map_err(|error| operation_error("v7 volume service restart failed", error))
    } else {
        Ok(())
    };

    match (backup, restart) {
        (Ok(backup), Ok(())) => Ok(backup),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(restart)) => Err(MigrationOperationError::new(format!(
            "{error}; v7 volume service restart also failed: {restart}"
        ))),
    }
}

async fn stream_backup(
    engine: &impl V7ContainerVolumeArchive,
    target: &V7ContainerCommandTarget,
    mounts: &[VolumeMount],
    options: &V7VolumeBackupOptions<'_>,
) -> Result<MigrationBackup, MigrationOperationError> {
    let (mut backup_reader, mut archive_output) = duplex(STREAM_BUFFER_BYTES);
    let download = async {
        let result = engine
            .download_v7_volume_archive(target, mounts, options.volume_name, &mut archive_output)
            .await;
        let close = archive_output.shutdown().await;
        result.map_err(|error| operation_error("v7 volume archive failed", error))?;
        close.map_err(|error| operation_error("v7 volume archive close failed", error))
    };
    let store = async {
        store_backup_artifact_from_async_reader(
            options.identity,
            &mut backup_reader,
            options.created_at_unix_seconds,
            options.backup_root,
        )
        .await
        .map_err(|error| operation_error("v7 volume backup storage failed", error))
    };
    let (_, stored) = tokio::time::timeout(
        options.timeout,
        futures_util::future::try_join(download, store),
    )
    .await
    .map_err(|_| MigrationOperationError::new("v7 volume backup timed out"))??;
    let evidence = verify_stored_backup_artifact(&stored, options.verified_at_unix_seconds)
        .map_err(|error| operation_error("v7 volume backup verification failed", error))?;
    let reference = stored.recovery_point().to_str().ok_or_else(|| {
        MigrationOperationError::new("v7 volume recovery point is not valid Unicode")
    })?;

    MigrationBackup::new(
        reference,
        evidence.artifact_sha256(),
        evidence.artifact_size_bytes(),
    )
}

fn validate(
    mounts: &[VolumeMount],
    options: &V7VolumeBackupOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let exact = mounts
        .iter()
        .filter(|mount| mount.source() == options.volume_name)
        .count()
        == 1;
    if !exact
        || options.created_at_unix_seconds < 0
        || options.verified_at_unix_seconds < options.created_at_unix_seconds
        || options.timeout.is_zero()
        || !options.backup_root.is_absolute()
    {
        return Err(MigrationOperationError::new(
            "v7 volume backup does not match exact accepted recovery identity",
        ));
    }

    Ok(())
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
