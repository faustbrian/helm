use super::V7MigrationAdapterPlan;
use std::path::Path;

/// Accepted identity needed to create the initial durable execution barrier.
pub(crate) struct V7MigrationExecutionPlanOptions<'plan> {
    pub(crate) project_id: &'plan str,
    pub(crate) canonical_project_path: &'plan Path,
    pub(crate) adapter_plan: &'plan V7MigrationAdapterPlan,
    pub(crate) planned_at_unix_seconds: i64,
}
