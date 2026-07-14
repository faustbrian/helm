use super::{V7MigrationAdapterRegistry, V7MigrationExecutionJournal};
use crate::control_plane::migration::MigrationCutoverPlan;
use crate::control_plane::state::V7MigrationExecutionRecord;

/// Complete boundaries and desired state for one project-wide v7 cutover.
pub(crate) struct V7MigrationCutoverOptions<'operation> {
    pub(crate) journal: &'operation mut dyn V7MigrationExecutionJournal,
    pub(crate) plan: &'operation V7MigrationExecutionRecord,
    pub(crate) registry: &'operation mut V7MigrationAdapterRegistry,
    pub(crate) desired_state: &'operation MigrationCutoverPlan,
    pub(crate) updated_at_unix_seconds: i64,
}
