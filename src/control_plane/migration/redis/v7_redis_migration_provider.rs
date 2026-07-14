use super::{
    RedisRestoreOptions, V7RedisMigrationProviderOptions, V7RedisSourceRetirement,
    backup_v7_redis_keyspace, restore_verified_redis_prefix, verify_v7_redis_source,
    verify_v7_redis_target,
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

pub(crate) struct V7RedisMigrationProvider<'operation, E, R> {
    executor: &'operation E,
    retirement: R,
    options: V7RedisMigrationProviderOptions<'operation>,
    source: V7LogicalDataMigrationSource,
    source_target: V7ContainerCommandTarget,
    backup_identity: BackupResourceIdentity,
    target_reference: String,
}

impl<'operation, E, R> V7RedisMigrationProvider<'operation, E, R>
where
    E: CommandExecutor + V7ContainerCommandExecutor + Sync,
    R: V7RedisSourceRetirement,
{
    pub(crate) fn new(
        executor: &'operation E,
        retirement: R,
        options: V7RedisMigrationProviderOptions<'operation>,
    ) -> Result<Self, MigrationOperationError> {
        validate_options(&options)?;
        let source_target = options.source.command_target().map_err(|error| {
            operation_error("v7 Redis-compatible command target is invalid", error)
        })?;
        let backup_identity = BackupResourceIdentity::for_v7_logical_data(
            options.source.project_id(),
            options.source.service_id(),
            options.source.driver(),
            options.accepted.evidence_revision(),
        );
        let target_reference = format!(
            "{}:{}:{}",
            options.flavor.implementation(),
            options.target_logical_resource.logical_resource_id(),
            options.target_acl.prefix()
        );
        Ok(Self {
            executor,
            retirement,
            source: options.source.clone(),
            source_target,
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
                "v7 Redis-compatible operation source differs from accepted provider identity",
            ));
        }
        Ok(())
    }

    fn verify_recovery(
        &self,
        checkpoint: &V7MigrationAdapterCheckpoint,
    ) -> Result<(StoredBackupArtifact, RecoveryPointRecord), MigrationOperationError> {
        let adapter_kind = format!("{}-tenant-prefix", self.options.flavor.implementation());
        if checkpoint.phase() != V7MigrationAdapterCheckpointPhase::RecoveryVerified
            || checkpoint.adapter_id() != format!("service/{}", self.source.service_id())
            || checkpoint.adapter_kind() != adapter_kind
            || !checkpoint.requires_recovery()
        {
            return Err(MigrationOperationError::new(
                "v7 Redis-compatible recovery checkpoint does not match the accepted source",
            ));
        }
        let reference = checkpoint.recovery_reference().ok_or_else(|| {
            MigrationOperationError::new("v7 Redis-compatible recovery checkpoint has no reference")
        })?;
        let stored = open_stored_backup_artifact(reference)
            .map_err(|error| operation_error("open v7 Redis-compatible recovery", error))?;
        let verified =
            verify_stored_backup_artifact(&stored, self.options.verified_at_unix_seconds)
                .map_err(|error| operation_error("verify v7 Redis-compatible recovery", error))?;
        if !verified.matches_identity(&self.backup_identity)
            || checkpoint.recovery_artifact_sha256() != Some(verified.artifact_sha256())
            || checkpoint.recovery_artifact_size_bytes() != Some(verified.artifact_size_bytes())
        {
            return Err(MigrationOperationError::new(
                "v7 Redis-compatible recovery differs from its durable checkpoint",
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
}

impl<E, R> V7RecoverableMigrationProvider<V7LogicalDataMigrationSource>
    for V7RedisMigrationProvider<'_, E, R>
where
    E: CommandExecutor + V7ContainerCommandExecutor + Sync,
    R: V7RedisSourceRetirement,
{
    fn backup_source<'operation>(
        &'operation mut self,
        source: &'operation V7LogicalDataMigrationSource,
    ) -> MigrationFuture<'operation, MigrationBackup> {
        Box::pin(async move {
            self.validate_source(source)?;
            backup_v7_redis_keyspace(
                self.executor,
                &self.source_target,
                self.options.flavor,
                self.options.source_credential,
                self.options.target_acl.prefix(),
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
            let options = self.restore_options(&recovery);
            restore_verified_redis_prefix(
                self.executor,
                self.options.target_container,
                &options,
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
                    "v8 Redis-compatible target reference differs from prepared prefix",
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
            verify_v7_redis_source(
                self.executor,
                &self.source_target,
                self.options.flavor,
                self.options.source_credential,
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

impl<E, R> V7RedisMigrationProvider<'_, E, R>
where
    E: CommandExecutor + V7ContainerCommandExecutor + Sync,
    R: V7RedisSourceRetirement,
{
    fn restore_options<'value>(
        &'value self,
        recovery: &'value RecoveryPointRecord,
    ) -> RedisRestoreOptions<'value> {
        RedisRestoreOptions {
            flavor: self.options.flavor,
            logical_resource: self.options.target_logical_resource,
            credential: self.options.target_credential,
            administrator: self.options.administrator,
            recovery_point: recovery,
            prefix: self.options.target_acl.prefix(),
            installation_id: self.options.installation_id,
            restored_at_unix_seconds: self.options.verified_at_unix_seconds,
            timeout: self.options.timeout,
        }
    }

    async fn verify_target_identity(&self) -> Result<(), MigrationOperationError> {
        verify_v7_redis_target(
            self.executor,
            self.options.target_container,
            self.options.flavor,
            self.options.target_acl,
            self.options.target_credential,
            self.options.timeout,
        )
        .await
    }
}

fn validate_options(
    options: &V7RedisMigrationProviderOptions<'_>,
) -> Result<(), MigrationOperationError> {
    crate::control_plane::migration::v7::validate_v7_logical_data_migration_source(
        options.accepted,
        options.source,
    )
    .map_err(MigrationOperationError::new)?;
    let implementation = options.flavor.implementation();
    let target = options.target_logical_resource;
    let fingerprint = target
        .compatibility_fingerprint()
        .strip_prefix("sha256:")
        .unwrap_or_default();
    let administrator_id = format!("shared/{fingerprint}/{implementation}-bootstrap");
    let expected_kind = format!("{implementation}_acl_prefix");
    let database_matches = match options.source.logical_data().get("database") {
        Some(value) => value.parse::<u32>().ok() == Some(options.source_credential.database()),
        None => {
            options.source.logical_data().is_empty() && options.source_credential.database() == 0
        }
    };
    let metadata = options.target_container.metadata();
    let invalid = options.source.driver() != implementation
        || options.source.kind() != "cache"
        || !database_matches
        || options.installation_id.is_empty()
        || !options.backup_root.is_absolute()
        || options.created_at_unix_seconds < 0
        || options.verified_at_unix_seconds < options.created_at_unix_seconds
        || options.timeout.is_zero()
        || target.kind() != expected_kind
        || target.lifecycle() != ResourceLifecycle::Active
        || target.project_id() != options.source.project_id()
        || target.service_id() != options.source.service_id()
        || target.logical_resource_id() != options.target_credential.credential_id()
        || options.target_credential.project_id() != Some(target.project_id())
        || options.target_credential.service_id() != target.service_id()
        || options.target_credential.lifecycle() != CredentialLifecycle::Active
        || !options
            .target_acl
            .matches_credential(options.target_credential)
        || options.administrator.credential_id() != administrator_id
        || options.administrator.project_id().is_some()
        || options.administrator.service_id() != implementation
        || options.administrator.username() != "stackctl_admin"
        || options.administrator.secret().is_empty()
        || options.administrator.lifecycle() != CredentialLifecycle::Active
        || fingerprint.len() != 64
        || metadata.installation_id() != options.installation_id
        || metadata.kind() != ResourceKind::SharedService
        || metadata.project_id().is_some()
        || metadata.compatibility_fingerprint() != target.compatibility_fingerprint();
    if invalid {
        return Err(MigrationOperationError::new(
            "v7 Redis-compatible provider does not describe one accepted source and owned v8 prefix",
        ));
    }
    Ok(())
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
