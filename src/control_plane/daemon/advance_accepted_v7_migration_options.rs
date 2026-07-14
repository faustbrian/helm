use super::RegisterAcceptedV7AdaptersOptions;
use crate::control_plane::migration::MigrationCutoverPlan;
use crate::control_plane::state::V7MigrationExecutionRecord;

/// Complete strategy and desired-state context for automatic v7 cutover.
pub(crate) struct AdvanceAcceptedV7MigrationOptions<'operation, E> {
    pub(crate) execution: &'operation V7MigrationExecutionRecord,
    pub(crate) registration: RegisterAcceptedV7AdaptersOptions<'operation, E>,
    pub(crate) desired_state: &'operation MigrationCutoverPlan,
    pub(crate) updated_at_unix_seconds: i64,
}
