use super::{V7MigrationAdapterExecutor, V7MigrationAdapterTarget};
use crate::control_plane::migration::{MigrationBackup, MigrationFuture, MigrationOperationError};
use crate::control_plane::state::V7MigrationAdapterCheckpoint;

/// Explicit strategy for a selected adapter that owns no external resource.
pub(crate) struct V7NoOpMigrationAdapter;

impl V7MigrationAdapterExecutor for V7NoOpMigrationAdapter {
    fn prepare_recovery<'operation>(
        &'operation mut self,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, MigrationBackup> {
        let adapter_id = checkpoint.adapter_id().to_owned();
        Box::pin(async move {
            Err(MigrationOperationError::new(format!(
                "no-op v7 adapter '{adapter_id}' cannot create recovery material"
            )))
        })
    }

    fn prepare_target<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, V7MigrationAdapterTarget> {
        Box::pin(async { Ok(V7MigrationAdapterTarget::NoExternalTarget) })
    }

    fn cutover<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }

    fn rollback<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }

    fn confirm<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }
}
