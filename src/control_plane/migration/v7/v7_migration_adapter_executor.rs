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

    /// Idempotently publishes this adapter's prepared target.
    fn cutover<'operation>(
        &'operation mut self,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()>;

    /// Idempotently restores source behavior and retains target evidence.
    fn rollback<'operation>(
        &'operation mut self,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()>;

    /// Idempotently retires this adapter's source after explicit confirmation.
    fn confirm<'operation>(
        &'operation mut self,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, ()>;
}
