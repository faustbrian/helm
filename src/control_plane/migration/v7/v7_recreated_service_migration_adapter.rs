use super::{V7MigrationAdapterExecutor, V7MigrationAdapterTarget};
use crate::control_plane::migration::{MigrationBackup, MigrationFuture, MigrationOperationError};
use crate::control_plane::state::V7MigrationAdapterCheckpoint;

/// Verifies a reconciled replacement whose lifecycle is owned by v8 desired state.
pub(super) struct V7RecreatedServiceMigrationAdapter {
    target: V7MigrationAdapterTarget,
}

impl V7RecreatedServiceMigrationAdapter {
    pub(super) const fn new(target: V7MigrationAdapterTarget) -> Self {
        Self { target }
    }
}

impl V7MigrationAdapterExecutor for V7RecreatedServiceMigrationAdapter {
    fn prepare_recovery<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, MigrationBackup> {
        Box::pin(async {
            Err(MigrationOperationError::new(
                "recreated v7 service does not own persistent recovery",
            ))
        })
    }

    fn prepare_target<'operation>(
        &'operation mut self,
        _checkpoint: &'operation V7MigrationAdapterCheckpoint,
    ) -> MigrationFuture<'operation, V7MigrationAdapterTarget> {
        let target = self.target.clone();
        Box::pin(async move { Ok(target) })
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
