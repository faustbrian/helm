use super::{V7MigrationAdapterTarget, V7NamedVolumeMigrationSource};
use crate::control_plane::migration::{MigrationBackup, MigrationFuture};
use crate::control_plane::state::V7MigrationAdapterCheckpoint;

/// Live provider for an ownership-checked legacy named-volume transition.
pub(crate) trait V7NamedVolumeMigrationProvider: Send {
    /// Quiesces the source, stores every exact volume, and restores source state.
    fn backup_source<'operation>(
        &'operation mut self,
        source: &'operation V7NamedVolumeMigrationSource,
    ) -> MigrationFuture<'operation, MigrationBackup>;

    /// Restores the verified archive and verifies the exact prepared v8 target.
    fn restore_and_verify_target<'operation>(
        &'operation mut self,
        source: &'operation V7NamedVolumeMigrationSource,
        checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, V7MigrationAdapterTarget>;

    fn verify_target<'operation>(
        &'operation mut self,
        source: &'operation V7NamedVolumeMigrationSource,
        target_reference: &'operation str,
    ) -> MigrationFuture<'operation, ()>;

    fn verify_source<'operation>(
        &'operation mut self,
        source: &'operation V7NamedVolumeMigrationSource,
    ) -> MigrationFuture<'operation, ()>;

    /// Retires only the exact accepted legacy source after project confirmation.
    fn retire_source<'operation>(
        &'operation mut self,
        source: &'operation V7NamedVolumeMigrationSource,
    ) -> MigrationFuture<'operation, ()>;
}
