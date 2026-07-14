use crate::control_plane::migration::{MigrationFuture, V7LogicalDataMigrationSource};

/// Confirmation-only cleanup strategy for one accepted v7 MinIO source.
pub(crate) trait V7MinioSourceRetirement: Send + Sync {
    fn retire_source<'operation>(
        &'operation mut self,
        source: &'operation V7LogicalDataMigrationSource,
    ) -> MigrationFuture<'operation, ()>;
}
