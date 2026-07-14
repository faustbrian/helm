use super::{
    V7SqlServerMigrationProviderOptions, V7SqlServerSourceRetirement,
    backup_v7_sql_server_database, restore_v7_sql_server_target, verify_v7_sql_server_source,
    verify_v7_sql_server_target,
};
use crate::control_plane::engine::{
    CommandExecutor, ResourceKind, RetentionClass, V7ContainerCommandExecutor,
    V7ContainerCommandTarget,
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
    CredentialLifecycle, ResourceLifecycle, V7MigrationAdapterCheckpoint,
    V7MigrationAdapterCheckpointPhase,
};

/// Live Engine-backed provider for one accepted v7 SQL Server database.
pub(crate) struct V7SqlServerMigrationProvider<'operation, E, R> {
    executor: &'operation E,
    retirement: &'operation mut R,
    options: V7SqlServerMigrationProviderOptions<'operation>,
    source: V7LogicalDataMigrationSource,
    source_target: V7ContainerCommandTarget,
    backup_identity: BackupResourceIdentity,
    target_reference: String,
}

impl<'operation, E, R> V7SqlServerMigrationProvider<'operation, E, R>
where
    E: CommandExecutor + V7ContainerCommandExecutor + Sync,
    R: V7SqlServerSourceRetirement,
{
    pub(crate) fn new(
        executor: &'operation E,
        retirement: &'operation mut R,
        options: V7SqlServerMigrationProviderOptions<'operation>,
    ) -> Result<Self, MigrationOperationError> {
        validate_options(&options)?;
        let source_target = options
            .source
            .command_target()
            .map_err(|error| operation_error("v7 SQL Server command target is invalid", error))?;
        let backup_identity = BackupResourceIdentity::for_v7_logical_data(
            options.source.project_id(),
            options.source.service_id(),
            options.source.driver(),
            options.accepted.evidence_revision(),
        );
        let target_reference = format!(
            "sqlserver:{}:{}",
            options.target_logical_resource.logical_resource_id(),
            options.target_plan.database_name()
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
                "v7 SQL Server operation source differs from accepted provider identity",
            ));
        }
        Ok(())
    }

    fn verify_recovery(
        &self,
        checkpoint: &V7MigrationAdapterCheckpoint,
    ) -> Result<StoredBackupArtifact, MigrationOperationError> {
        if checkpoint.phase() != V7MigrationAdapterCheckpointPhase::RecoveryVerified
            || checkpoint.adapter_id() != format!("service/{}", self.source.service_id())
            || checkpoint.adapter_kind() != "sqlserver-logical-database"
            || !checkpoint.requires_recovery()
        {
            return Err(MigrationOperationError::new(
                "v7 SQL Server recovery checkpoint does not match the accepted source",
            ));
        }
        let reference = checkpoint.recovery_reference().ok_or_else(|| {
            MigrationOperationError::new("v7 SQL Server recovery checkpoint has no reference")
        })?;
        let stored = open_stored_backup_artifact(reference)
            .map_err(|error| operation_error("open v7 SQL Server recovery", error))?;
        let verified =
            verify_stored_backup_artifact(&stored, self.options.verified_at_unix_seconds)
                .map_err(|error| operation_error("verify v7 SQL Server recovery", error))?;
        if !verified.matches_identity(&self.backup_identity)
            || checkpoint.recovery_artifact_sha256() != Some(verified.artifact_sha256())
            || checkpoint.recovery_artifact_size_bytes() != Some(verified.artifact_size_bytes())
        {
            return Err(MigrationOperationError::new(
                "v7 SQL Server recovery differs from its durable checkpoint",
            ));
        }
        Ok(stored)
    }
}

impl<E, R> V7RecoverableMigrationProvider<V7LogicalDataMigrationSource>
    for V7SqlServerMigrationProvider<'_, E, R>
where
    E: CommandExecutor + V7ContainerCommandExecutor + Sync,
    R: V7SqlServerSourceRetirement,
{
    fn backup_source<'operation>(
        &'operation mut self,
        source: &'operation V7LogicalDataMigrationSource,
    ) -> MigrationFuture<'operation, MigrationBackup> {
        Box::pin(async move {
            self.validate_source(source)?;
            backup_v7_sql_server_database(
                self.executor,
                &self.source_target,
                self.options.source_credential,
                source_database_name(source)?,
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
            let stored = self.verify_recovery(checkpoint)?;
            restore_v7_sql_server_target(
                self.executor,
                self.options.target_container,
                self.options.target_plan,
                self.options.administrator,
                self.options.target_credential,
                &stored,
                self.options.timeout,
            )
            .await?;
            verify_v7_sql_server_target(
                self.executor,
                self.options.target_container,
                self.options.target_plan,
                self.options.target_credential,
                self.options.timeout,
            )
            .await?;
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
                    "v8 SQL Server target reference differs from prepared resource",
                ));
            }
            verify_v7_sql_server_target(
                self.executor,
                self.options.target_container,
                self.options.target_plan,
                self.options.target_credential,
                self.options.timeout,
            )
            .await
        })
    }

    fn verify_source<'operation>(
        &'operation mut self,
        source: &'operation V7LogicalDataMigrationSource,
    ) -> MigrationFuture<'operation, ()> {
        Box::pin(async move {
            self.validate_source(source)?;
            verify_v7_sql_server_source(
                self.executor,
                &self.source_target,
                self.options.source_credential,
                source_database_name(source)?,
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

fn validate_options(
    options: &V7SqlServerMigrationProviderOptions<'_>,
) -> Result<(), MigrationOperationError> {
    crate::control_plane::migration::v7::validate_v7_logical_data_migration_source(
        options.accepted,
        options.source,
    )
    .map_err(MigrationOperationError::new)?;
    let target = options.target_logical_resource;
    let metadata = options.target_container.metadata();
    let administrator_id = format!(
        "migration/{}/sqlserver-bootstrap",
        metadata.resource_id().unwrap_or_default()
    );
    let invalid = !matches!(options.source.driver(), "sqlserver" | "mssql")
        || options.source.kind() != "database"
        || source_database_name(options.source)?.is_empty()
        || options.source_credential.username() != "sa"
        || options.installation_id.is_empty()
        || !options.backup_root.is_absolute()
        || options.created_at_unix_seconds < 0
        || options.verified_at_unix_seconds < options.created_at_unix_seconds
        || options.timeout.is_zero()
        || target.kind() != "sqlserver_database"
        || target.lifecycle() != ResourceLifecycle::Active
        || target.project_id() != options.source.project_id()
        || target.service_id() != options.source.service_id()
        || target.logical_resource_id()
            != format!("{}/{}", target.project_id(), target.service_id())
        || options.target_credential.project_id() != Some(target.project_id())
        || options.target_credential.service_id() != target.service_id()
        || options.target_credential.lifecycle() != CredentialLifecycle::Active
        || !options
            .target_plan
            .matches_credential(options.target_credential)
        || options.administrator.credential_id() != administrator_id
        || options.administrator.project_id() != Some(target.project_id())
        || options.administrator.service_id() != "sqlserver"
        || options.administrator.username() != "sa"
        || options.administrator.secret().is_empty()
        || options.administrator.lifecycle() != CredentialLifecycle::Active
        || metadata.installation_id() != options.installation_id
        || metadata.kind() != ResourceKind::ProjectService
        || metadata.project_id() != Some(target.project_id())
        || metadata.resource_id().is_none()
        || metadata.retention() != RetentionClass::Persistent
        || metadata.compatibility_fingerprint() != target.compatibility_fingerprint();
    if invalid {
        return Err(MigrationOperationError::new(
            "v7 SQL Server provider does not describe one accepted source and owned v8 target",
        ));
    }
    Ok(())
}

fn source_database_name(
    source: &V7LogicalDataMigrationSource,
) -> Result<&str, MigrationOperationError> {
    if source.logical_data().len() != 1 {
        return Err(MigrationOperationError::new(
            "v7 SQL Server source requires exactly one accepted database identity",
        ));
    }
    let database = source
        .logical_data()
        .get("database")
        .map(String::as_str)
        .ok_or_else(|| {
            MigrationOperationError::new("v7 SQL Server source has no accepted database identity")
        })?;
    if !valid_identifier(database) {
        return Err(MigrationOperationError::new(
            "v7 SQL Server source database is not a safe SQL identifier",
        ));
    }
    Ok(database)
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().enumerate().all(|(index, byte)| match byte {
            b'a'..=b'z' | b'_' => true,
            b'0'..=b'9' => index > 0,
            _ => false,
        })
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
