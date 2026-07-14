use super::AcceptedV7EnvironmentRollback;
use std::path::PathBuf;

/// Complete secret-free evidence accepted for one legacy project source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AcceptedV7InventoryRecordOptions {
    pub(crate) project_id: String,
    pub(crate) canonical_project_path: PathBuf,
    pub(crate) source_revision: String,
    pub(crate) inventory_json: String,
    pub(crate) generated_environment_rollback: Option<AcceptedV7EnvironmentRollback>,
    pub(crate) accepted_at_unix_seconds: i64,
}
