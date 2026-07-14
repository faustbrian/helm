use super::V7MigrationAdapterTarget;
use crate::control_plane::migration::{MigrationBackup, MigrationFuture};
use crate::control_plane::state::V7MigrationAdapterCheckpoint;

/// Idempotent strategy boundary for one selected v7 adapter kind.
pub(crate) trait V7MigrationAdapterExecutor: Send {
    fn prepare_recovery<'operation>(
        &'operation mut self,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, MigrationBackup>;

    fn prepare_target<'operation>(
        &'operation mut self,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, V7MigrationAdapterTarget>;
}
