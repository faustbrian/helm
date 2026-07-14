use crate::control_plane::migration::MigrationFuture;
use crate::control_plane::state::{LogicalResourceRecord, MigrationRecord};

/// Explicit, idempotent retirement of one retained PostgreSQL source.
pub(crate) trait PostgresSourceRetirement {
    fn retire_source<'operation>(
        &'operation mut self,
        inventory: &'operation MigrationRecord,
        checkpoint: &'operation MigrationRecord,
        source: &'operation LogicalResourceRecord,
    ) -> MigrationFuture<'operation, ()>;
}
