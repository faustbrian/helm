use super::{V7MigrationAdapterRegistry, V7MigrationExecutionJournal};
use crate::control_plane::migration::MigrationRollbackPlan;
use crate::control_plane::state::V7MigrationExecutionRecord;

/// Complete boundaries and restored state for one project-wide v7 rollback.
pub(crate) struct V7MigrationRollbackOptions<'operation, 'adapter> {
    pub(crate) journal: &'operation mut dyn V7MigrationExecutionJournal,
    pub(crate) plan: &'operation V7MigrationExecutionRecord,
    pub(crate) registry: &'operation mut V7MigrationAdapterRegistry<'adapter>,
    pub(crate) restored_state: &'operation MigrationRollbackPlan,
    pub(crate) updated_at_unix_seconds: i64,
}
