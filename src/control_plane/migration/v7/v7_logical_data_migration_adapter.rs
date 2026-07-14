use super::{
    V7LogicalDataMigrationAdapterOptions, V7LogicalDataMigrationSource, V7MigrationAdapterExecutor,
    V7MigrationAdapterTarget, V7RecoverableMigrationProvider,
    validate_v7_logical_data_migration_source,
};
use crate::control_plane::migration::{MigrationBackup, MigrationFuture, MigrationOperationError};
use crate::control_plane::state::V7MigrationAdapterCheckpoint;

/// Runs one accepted logical-data transition through its driver provider.
pub(super) struct V7LogicalDataMigrationAdapter<'operation> {
    source: V7LogicalDataMigrationSource,
    provider: Box<dyn V7RecoverableMigrationProvider<V7LogicalDataMigrationSource> + 'operation>,
}

impl<'operation> V7LogicalDataMigrationAdapter<'operation> {
    pub(super) fn new(
        options: V7LogicalDataMigrationAdapterOptions<'operation>,
    ) -> Result<Self, String> {
        validate_v7_logical_data_migration_source(options.accepted, options.source)?;

        Ok(Self {
            source: options.source.clone(),
            provider: options.provider,
        })
    }
}

impl V7MigrationAdapterExecutor for V7LogicalDataMigrationAdapter<'_> {
    fn prepare_recovery<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, MigrationBackup> {
        self.provider.backup_source(&self.source)
    }

    fn prepare_target<'operation>(
        &'operation mut self,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, V7MigrationAdapterTarget> {
        if checkpoint.recovery_reference().is_none()
            || checkpoint.recovery_artifact_sha256().is_none()
            || checkpoint.recovery_artifact_size_bytes().is_none()
        {
            return Box::pin(async {
                Err(MigrationOperationError::new(
                    "logical-data target requires verified recovery evidence",
                ))
            });
        }
        self.provider
            .restore_and_verify_target(&self.source, checkpoint)
    }

    fn cutover<'operation>(
        &'operation mut self,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        let Some(target_reference) = checkpoint.target_reference() else {
            return Box::pin(async {
                Err(MigrationOperationError::new(
                    "logical-data checkpoint has no prepared target",
                ))
            });
        };
        self.provider.verify_target(&self.source, target_reference)
    }

    fn rollback<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        self.provider.verify_source(&self.source)
    }

    fn confirm<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        self.provider.retire_source(&self.source)
    }
}
