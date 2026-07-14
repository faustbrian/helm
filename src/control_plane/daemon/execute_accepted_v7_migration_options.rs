use super::AcceptedV7MigrationAction;
use crate::control_plane::migration::{V7MigrationAdapterRegistry, V7MigrationExecutionJournal};
use crate::control_plane::state::V7MigrationExecutionRecord;

/// Complete durable context for one project-wide accepted-v7 transition.
pub(crate) struct ExecuteAcceptedV7MigrationOptions<'operation, 'adapter> {
    pub(crate) journal: &'operation mut dyn V7MigrationExecutionJournal,
    pub(crate) plan: &'operation V7MigrationExecutionRecord,
    pub(crate) registry: &'operation mut V7MigrationAdapterRegistry<'adapter>,
    pub(crate) action: AcceptedV7MigrationAction<'operation>,
    pub(crate) updated_at_unix_seconds: i64,
}
