use super::{
    V7MigrationAdapterTarget, V7NamedVolumeMigrationProviderOptions, V7NamedVolumeMigrationSource,
    V7RecoverableMigrationProvider,
};
use crate::control_plane::engine::{
    ContainerHealth, ContainerLifecycle, ContainerState, ContainerVolumeArchive, HealthObserver,
    OwnedContainer, OwnedVolume, V7ContainerCommandTarget, V7ContainerRetirement,
    V7ContainerRetirementTarget, V7ContainerVolumeArchive, VolumeManager, VolumeMount,
};
use crate::control_plane::migration::{
    MigrationBackup, MigrationFuture, MigrationOperationError, V7VolumeBackupOptions,
    V7VolumeTargetRestoreOptions, backup_v7_volume, restore_v7_volume_target,
};
use crate::control_plane::retention::{
    BackupResourceIdentity, StoredBackupArtifact, open_stored_backup_artifact,
    verify_stored_backup_artifact,
};
use crate::control_plane::state::{
    V7MigrationAdapterCheckpoint, V7MigrationAdapterCheckpointPhase,
};
use std::path::PathBuf;
use std::time::Duration;

/// Live Engine-backed provider for one exact accepted-v7 named volume.
pub(crate) struct V7NamedVolumeMigrationProvider<E> {
    engine: E,
    source: V7NamedVolumeMigrationSource,
    source_target: V7ContainerCommandTarget,
    source_mounts: Vec<VolumeMount>,
    target_container: OwnedContainer,
    target_volume: OwnedVolume,
    desired_container: crate::control_plane::engine::ContainerCreateOptions,
    desired_volume: crate::control_plane::engine::VolumeCreateOptions,
    backup_identity: BackupResourceIdentity,
    backup_root: PathBuf,
    created_at_unix_seconds: i64,
    verified_at_unix_seconds: i64,
    timeout: Duration,
    target_reference: String,
}

impl<E> V7NamedVolumeMigrationProvider<E> {
    pub(crate) fn new(
        engine: E,
        options: V7NamedVolumeMigrationProviderOptions<'_>,
    ) -> Result<Self, MigrationOperationError> {
        let source_target = options
            .source
            .command_target()
            .map_err(|error| operation_error("v7 volume command target is invalid", error))?;
        let source_mounts = options
            .source
            .mounts()
            .iter()
            .map(|mount| VolumeMount::read_write(mount.volume_name(), mount.target()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| operation_error("v7 volume mount identity is invalid", error))?;
        validate_options(&options, &source_mounts)?;
        let desired_volume = options
            .target_plan
            .volume()
            .expect("validated retained target volume")
            .clone();
        let target_reference = format!("volume:{}", desired_volume.name());

        Ok(Self {
            engine,
            source: options.source.clone(),
            source_target,
            source_mounts,
            target_container: options.target_container.clone(),
            target_volume: options.target_volume.clone(),
            desired_container: options.target_plan.request().clone(),
            desired_volume,
            backup_identity: BackupResourceIdentity::for_v7_named_volume(
                options.accepted.project_id(),
                options.source.service_id(),
                options.accepted.evidence_revision(),
            ),
            backup_root: options.backup_root.to_path_buf(),
            created_at_unix_seconds: options.created_at_unix_seconds,
            verified_at_unix_seconds: options.verified_at_unix_seconds,
            timeout: options.timeout,
            target_reference,
        })
    }

    fn validate_source(
        &self,
        source: &V7NamedVolumeMigrationSource,
    ) -> Result<(), MigrationOperationError> {
        if source != &self.source {
            return Err(MigrationOperationError::new(
                "v7 volume operation source differs from the accepted provider identity",
            ));
        }

        Ok(())
    }

    fn verify_recovery(
        &self,
        checkpoint: &V7MigrationAdapterCheckpoint,
    ) -> Result<StoredBackupArtifact, MigrationOperationError> {
        if checkpoint.phase() != V7MigrationAdapterCheckpointPhase::RecoveryVerified
            || checkpoint.adapter_id() != format!("volume/{}", self.source.service_id())
            || checkpoint.adapter_kind() != "named-volume-archive"
            || !checkpoint.requires_recovery()
        {
            return Err(MigrationOperationError::new(
                "v7 volume recovery checkpoint does not match the accepted source",
            ));
        }
        let reference = checkpoint.recovery_reference().ok_or_else(|| {
            MigrationOperationError::new("v7 volume recovery checkpoint has no reference")
        })?;
        let stored = open_stored_backup_artifact(reference)
            .map_err(|error| operation_error("open v7 volume recovery", error))?;
        let evidence = verify_stored_backup_artifact(&stored, self.verified_at_unix_seconds)
            .map_err(|error| operation_error("verify v7 volume recovery", error))?;
        if !evidence.matches_identity(&self.backup_identity)
            || checkpoint.recovery_artifact_sha256() != Some(evidence.artifact_sha256())
            || checkpoint.recovery_artifact_size_bytes() != Some(evidence.artifact_size_bytes())
        {
            return Err(MigrationOperationError::new(
                "v7 volume recovery artifact differs from its durable checkpoint",
            ));
        }

        Ok(stored)
    }
}

impl<E> V7RecoverableMigrationProvider<V7NamedVolumeMigrationSource>
    for V7NamedVolumeMigrationProvider<E>
where
    E: ContainerLifecycle
        + ContainerVolumeArchive
        + HealthObserver
        + V7ContainerRetirement
        + V7ContainerVolumeArchive
        + VolumeManager
        + Send
        + Sync,
{
    fn backup_source<'operation>(
        &'operation mut self,
        source: &'operation V7NamedVolumeMigrationSource,
    ) -> MigrationFuture<'operation, MigrationBackup> {
        Box::pin(async move {
            self.validate_source(source)?;
            backup_v7_volume(
                &self.engine,
                &self.source_target,
                &self.source_mounts,
                &V7VolumeBackupOptions {
                    identity: &self.backup_identity,
                    volume_name: self.source.mounts()[0].volume_name(),
                    backup_root: &self.backup_root,
                    created_at_unix_seconds: self.created_at_unix_seconds,
                    verified_at_unix_seconds: self.verified_at_unix_seconds,
                    timeout: self.timeout,
                },
            )
            .await
        })
    }

    fn restore_and_verify_target<'operation>(
        &'operation mut self,
        source: &'operation V7NamedVolumeMigrationSource,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, V7MigrationAdapterTarget> {
        Box::pin(async move {
            self.validate_source(source)?;
            let stored = self.verify_recovery(checkpoint)?;
            let (container, volume) = restore_v7_volume_target(
                &mut self.engine,
                &self.target_container,
                &self.target_volume,
                &V7VolumeTargetRestoreOptions {
                    desired_container: &self.desired_container,
                    desired_volume: &self.desired_volume,
                    expected_mount_target: self.source.mounts()[0].target(),
                    archive: stored.artifact_file(),
                    timeout: self.timeout,
                },
            )
            .await?;
            self.target_container = container;
            self.target_volume = volume;
            verify_target(self).await?;
            V7MigrationAdapterTarget::resource(&self.target_reference)
                .map_err(MigrationOperationError::new)
        })
    }

    fn verify_target<'operation>(
        &'operation mut self,
        source: &'operation V7NamedVolumeMigrationSource,
        target_reference: &'operation str,
    ) -> MigrationFuture<'operation, ()> {
        Box::pin(async move {
            self.validate_source(source)?;
            if target_reference != self.target_reference {
                return Err(MigrationOperationError::new(
                    "v7 volume target reference differs from the prepared target",
                ));
            }
            verify_target(self).await
        })
    }

    fn verify_source<'operation>(
        &'operation mut self,
        source: &'operation V7NamedVolumeMigrationSource,
    ) -> MigrationFuture<'operation, ()> {
        Box::pin(async move {
            self.validate_source(source)?;
            if self
                .engine
                .inspect_v7_volume_container(&self.source_target, &self.source_mounts)
                .await
                .map_err(|error| operation_error("verify v7 volume source", error))?
                == ContainerState::Missing
            {
                return Err(MigrationOperationError::new(
                    "accepted v7 volume source is missing",
                ));
            }

            Ok(())
        })
    }

    fn retire_source<'operation>(
        &'operation mut self,
        source: &'operation V7NamedVolumeMigrationSource,
    ) -> MigrationFuture<'operation, ()> {
        Box::pin(async move {
            self.validate_source(source)?;
            let target = V7ContainerRetirementTarget::new(
                self.source_target.clone(),
                self.source.volume_names().to_vec(),
            )
            .map_err(|error| operation_error("v7 volume retirement target is invalid", error))?;
            self.engine
                .retire_v7_container(&target)
                .await
                .map_err(|error| operation_error("retire v7 volume source", error))
        })
    }
}

async fn verify_target<E>(
    provider: &V7NamedVolumeMigrationProvider<E>,
) -> Result<(), MigrationOperationError>
where
    E: ContainerLifecycle + HealthObserver,
{
    if provider
        .engine
        .inspect(&provider.target_container)
        .await
        .map_err(|error| operation_error("verify v8 volume target state", error))?
        != ContainerState::Running
    {
        return Err(MigrationOperationError::new(
            "prepared v8 volume target is not running",
        ));
    }
    match provider
        .engine
        .observe_health(&provider.target_container)
        .await
        .map_err(|error| operation_error("verify v8 volume target health", error))?
    {
        ContainerHealth::Healthy => Ok(()),
        ContainerHealth::RunningUnverified
            if provider.desired_container.health_check().is_none() =>
        {
            Ok(())
        }
        _ => Err(MigrationOperationError::new(
            "prepared v8 volume target is not ready",
        )),
    }
}

fn validate_options(
    options: &V7NamedVolumeMigrationProviderOptions<'_>,
    source_mounts: &[VolumeMount],
) -> Result<(), MigrationOperationError> {
    let desired_volume = options.target_plan.volume().ok_or_else(|| {
        MigrationOperationError::new("v7 named volume has no retained v8 target volume")
    })?;
    let target_mounts = options.target_plan.request().volume_mounts();
    let valid = options.accepted.project_id()
        == options
            .target_container
            .metadata()
            .project_id()
            .unwrap_or_default()
        && options.source.service_id()
            == options
                .target_container
                .metadata()
                .resource_id()
                .unwrap_or_default()
        && source_mounts.len() == 1
        && target_mounts.len() == 1
        && source_mounts[0].target() == target_mounts[0].target()
        && target_mounts[0].source() == desired_volume.name()
        && options.target_container.metadata() == options.target_plan.request().metadata()
        && options.target_volume.name() == desired_volume.name()
        && options.target_volume.metadata() == desired_volume.metadata()
        && options.created_at_unix_seconds >= 0
        && options.verified_at_unix_seconds >= options.created_at_unix_seconds
        && options.backup_root.is_absolute()
        && !options.timeout.is_zero();
    if !valid {
        return Err(MigrationOperationError::new(
            "v7 named-volume provider requires one exact compatible prepared-v8 mount mapping",
        ));
    }

    Ok(())
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
