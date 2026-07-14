use super::V7MigrationAdapterTarget;
use crate::control_plane::migration::{MigrationBackup, MigrationFuture};
use crate::control_plane::state::V7MigrationAdapterCheckpoint;

/// Shared lifecycle for a recovery-first transition with a retained source.
pub(crate) trait V7RecoverableMigrationProvider<Source>: Send {
    fn backup_source<'operation>(
        &'operation mut self,
        source: &'operation Source,
    ) -> MigrationFuture<'operation, MigrationBackup>;

    /// Restores recovery evidence and verifies the exact prepared v8 target.
    fn restore_and_verify_target<'operation>(
        &'operation mut self,
        source: &'operation Source,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, V7MigrationAdapterTarget>;

    fn verify_target<'operation>(
        &'operation mut self,
        source: &'operation Source,
        target_reference: &'operation str,
    ) -> MigrationFuture<'operation, ()>;

    fn verify_source<'operation>(
        &'operation mut self,
        source: &'operation Source,
    ) -> MigrationFuture<'operation, ()>;

    /// Retires only the exact accepted source after project confirmation.
    fn retire_source<'operation>(
        &'operation mut self,
        source: &'operation Source,
    ) -> MigrationFuture<'operation, ()>;
}
