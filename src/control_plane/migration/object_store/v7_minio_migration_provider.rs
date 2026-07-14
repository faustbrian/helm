use super::{
    MinioRestoreOptions, V7MinioMigrationProviderOptions, V7MinioSourceRetirement,
    backup_v7_minio_bucket, restore_verified_minio_bucket, verify_v7_minio_source,
    verify_v7_minio_target,
};
use crate::control_plane::engine::{
    CommandExecutor, ResourceKind, V7ContainerCommandExecutor, V7ContainerCommandTarget,
};
use crate::control_plane::migration::{
    MigrationBackup, MigrationFuture, MigrationOperationError, V7LogicalDataMigrationSource,
    V7MigrationAdapterTarget, V7RecoverableMigrationProvider,
};
use crate::control_plane::retention::{
    BackupResourceIdentity, StoredBackupArtifact, open_stored_backup_artifact,
    verify_stored_backup_artifact,
};
use crate::control_plane::state::{
    CredentialLifecycle, RecoveryPointRecord, RecoveryPointRecordOptions, ResourceLifecycle,
    V7MigrationAdapterCheckpoint, V7MigrationAdapterCheckpointPhase,
};

pub(crate) struct V7MinioMigrationProvider<'operation, E, R> {
    executor: &'operation E,
    retirement: R,
    options: V7MinioMigrationProviderOptions<'operation>,
    source: V7LogicalDataMigrationSource,
    source_target: V7ContainerCommandTarget,
    source_bucket: String,
    backup_identity: BackupResourceIdentity,
    target_reference: String,
}

impl<'operation, E, R> V7MinioMigrationProvider<'operation, E, R>
where
    E: CommandExecutor + V7ContainerCommandExecutor + Sync,
    R: V7MinioSourceRetirement,
{
    pub(crate) fn new(
        executor: &'operation E,
        retirement: R,
        options: V7MinioMigrationProviderOptions<'operation>,
    ) -> Result<Self, MigrationOperationError> {
        let source_bucket = validate_options(&options)?.to_owned();
        let source_target = options
            .source
            .command_target()
            .map_err(|error| operation_error("v7 MinIO command target is invalid", error))?;
        let backup_identity = BackupResourceIdentity::for_v7_logical_data(
            options.source.project_id(),
            options.source.service_id(),
            options.source.driver(),
            options.accepted.evidence_revision(),
        );
        let target_reference = format!(
            "minio:{}:{}",
            options.target_logical_resource.logical_resource_id(),
            options.target_definition.bucket()
        );
        Ok(Self {
            executor,
            retirement,
            source: options.source.clone(),
            source_target,
            source_bucket,
            backup_identity,
            target_reference,
            options,
        })
    }

    fn validate_source(
        &self,
        source: &V7LogicalDataMigrationSource,
    ) -> Result<(), MigrationOperationError> {
        if source != &self.source {
            return Err(MigrationOperationError::new(
                "v7 MinIO operation source differs from accepted provider identity",
            ));
        }
        Ok(())
    }

    fn verify_recovery(
        &self,
        checkpoint: &V7MigrationAdapterCheckpoint,
    ) -> Result<(StoredBackupArtifact, RecoveryPointRecord), MigrationOperationError> {
        if checkpoint.phase() != V7MigrationAdapterCheckpointPhase::RecoveryVerified
            || checkpoint.adapter_id() != format!("service/{}", self.source.service_id())
            || checkpoint.adapter_kind() != "minio-bucket"
            || !checkpoint.requires_recovery()
        {
            return Err(MigrationOperationError::new(
                "v7 MinIO recovery checkpoint does not match the accepted source",
            ));
        }
        let reference = checkpoint.recovery_reference().ok_or_else(|| {
            MigrationOperationError::new("v7 MinIO recovery checkpoint has no reference")
        })?;
        let stored = open_stored_backup_artifact(reference)
            .map_err(|error| operation_error("open v7 MinIO recovery", error))?;
        let verified =
            verify_stored_backup_artifact(&stored, self.options.verified_at_unix_seconds)
                .map_err(|error| operation_error("verify v7 MinIO recovery", error))?;
        if !verified.matches_identity(&self.backup_identity)
            || checkpoint.recovery_artifact_sha256() != Some(verified.artifact_sha256())
            || checkpoint.recovery_artifact_size_bytes() != Some(verified.artifact_size_bytes())
        {
            return Err(MigrationOperationError::new(
                "v7 MinIO recovery differs from its durable checkpoint",
            ));
        }
        let logical = self.options.target_logical_resource;
        let recovery = RecoveryPointRecord::new(RecoveryPointRecordOptions {
            recovery_point_id: format!(
                "v7-{}-{}",
                self.source.project_id(),
                self.source.service_id()
            ),
            project_id: logical.project_id().to_owned(),
            service_id: logical.service_id().to_owned(),
            logical_resource_id: logical.logical_resource_id().to_owned(),
            resource_kind: logical.kind().to_owned(),
            compatibility_fingerprint: logical.compatibility_fingerprint().to_owned(),
            reference: reference.to_owned(),
            artifact_sha256: verified.artifact_sha256().to_owned(),
            artifact_size_bytes: verified.artifact_size_bytes(),
            created_at_unix_seconds: self.options.created_at_unix_seconds,
            verified_at_unix_seconds: self.options.verified_at_unix_seconds,
        })
        .map_err(MigrationOperationError::new)?;
        Ok((stored, recovery))
    }

    fn restore_options<'value>(
        &'value self,
        recovery: &'value RecoveryPointRecord,
    ) -> MinioRestoreOptions<'value> {
        MinioRestoreOptions {
            recovery_point: recovery,
            logical_resource: self.options.target_logical_resource,
            credential: self.options.target_credential,
            installation_id: self.options.installation_id,
            target_bucket_name: self.options.target_definition.bucket(),
            verified_at_unix_seconds: self.options.verified_at_unix_seconds,
            timeout: self.options.timeout,
        }
    }

    async fn verify_target_identity(&self) -> Result<(), MigrationOperationError> {
        verify_v7_minio_target(
            self.executor,
            self.options.target_container,
            self.options.target_definition,
            self.options.target_credential,
            self.options.timeout,
        )
        .await
    }
}

impl<E, R> V7RecoverableMigrationProvider<V7LogicalDataMigrationSource>
    for V7MinioMigrationProvider<'_, E, R>
where
    E: CommandExecutor + V7ContainerCommandExecutor + Sync,
    R: V7MinioSourceRetirement,
{
    fn backup_source<'operation>(
        &'operation mut self,
        source: &'operation V7LogicalDataMigrationSource,
    ) -> MigrationFuture<'operation, MigrationBackup> {
        Box::pin(async move {
            self.validate_source(source)?;
            backup_v7_minio_bucket(
                self.executor,
                &self.source_target,
                self.options.source_credential,
                &self.source_bucket,
                &self.backup_identity,
                self.options.backup_root,
                self.options.created_at_unix_seconds,
                self.options.verified_at_unix_seconds,
                self.options.timeout,
            )
            .await
        })
    }

    fn restore_and_verify_target<'operation>(
        &'operation mut self,
        source: &'operation V7LogicalDataMigrationSource,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, V7MigrationAdapterTarget> {
        Box::pin(async move {
            self.validate_source(source)?;
            let (stored, recovery) = self.verify_recovery(checkpoint)?;
            restore_verified_minio_bucket(
                self.executor,
                self.options.target_container,
                &self.restore_options(&recovery),
                &stored,
            )
            .await?;
            self.verify_target_identity().await?;
            V7MigrationAdapterTarget::resource(&self.target_reference)
                .map_err(MigrationOperationError::new)
        })
    }

    fn verify_target<'operation>(
        &'operation mut self,
        source: &'operation V7LogicalDataMigrationSource,
        target_reference: &'operation str,
    ) -> MigrationFuture<'operation, ()> {
        Box::pin(async move {
            self.validate_source(source)?;
            if target_reference != self.target_reference {
                return Err(MigrationOperationError::new(
                    "v8 MinIO target reference differs from prepared bucket",
                ));
            }
            self.verify_target_identity().await
        })
    }

    fn verify_source<'operation>(
        &'operation mut self,
        source: &'operation V7LogicalDataMigrationSource,
    ) -> MigrationFuture<'operation, ()> {
        Box::pin(async move {
            self.validate_source(source)?;
            verify_v7_minio_source(
                self.executor,
                &self.source_target,
                self.options.source_credential,
                &self.source_bucket,
                self.options.timeout,
            )
            .await
        })
    }

    fn retire_source<'operation>(
        &'operation mut self,
        source: &'operation V7LogicalDataMigrationSource,
    ) -> MigrationFuture<'operation, ()> {
        if let Err(error) = self.validate_source(source) {
            return Box::pin(async move { Err(error) });
        }
        self.retirement.retire_source(source)
    }
}

fn validate_options<'operation>(
    options: &'operation V7MinioMigrationProviderOptions<'operation>,
) -> Result<&'operation str, MigrationOperationError> {
    crate::control_plane::migration::v7::validate_v7_logical_data_migration_source(
        options.accepted,
        options.source,
    )
    .map_err(MigrationOperationError::new)?;
    let bucket = options
        .source
        .logical_data()
        .get("bucket")
        .map(String::as_str)
        .unwrap_or_default();
    let target = options.target_logical_resource;
    let credential = options.target_credential;
    let definition = options.target_definition;
    let metadata = options.target_container.metadata();
    let invalid = options.source.driver() != "minio"
        || options.source.kind() != "object_store"
        || options.source.logical_data().len() != 1
        || !valid_bucket(bucket)
        || options.installation_id.is_empty()
        || !options.backup_root.is_absolute()
        || options.created_at_unix_seconds < 0
        || options.verified_at_unix_seconds < options.created_at_unix_seconds
        || options.timeout.is_zero()
        || target.kind() != "minio_bucket_policy"
        || target.lifecycle() != ResourceLifecycle::Active
        || target.project_id() != options.source.project_id()
        || target.service_id() != options.source.service_id()
        || target.logical_resource_id() != credential.credential_id()
        || credential.project_id() != Some(target.project_id())
        || credential.service_id() != target.service_id()
        || credential.lifecycle() != CredentialLifecycle::Active
        || !definition.matches_credential(credential)
        || definition.bucket()
            != format!("stackctl-{}-{}", target.project_id(), target.service_id())
        || metadata.installation_id() != options.installation_id
        || metadata.kind() != ResourceKind::SharedService
        || metadata.project_id().is_some()
        || metadata.compatibility_fingerprint() != target.compatibility_fingerprint();
    if invalid {
        return Err(MigrationOperationError::new(
            "v7 MinIO provider does not describe one accepted source and owned v8 bucket",
        ));
    }
    Ok(bucket)
}

fn valid_bucket(bucket: &str) -> bool {
    let bytes = bucket.as_bytes();
    (3..=63).contains(&bytes.len())
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
        })
        && !bucket.contains("..")
        && !bucket.contains(".-")
        && !bucket.contains("-.")
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
