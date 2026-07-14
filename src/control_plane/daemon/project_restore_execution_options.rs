use super::QueuedProjectRestore;
use crate::control_plane::shared_infrastructure::SharedInstancePlan;
use std::path::PathBuf;
use std::time::Duration;

/// Complete owned context for one asynchronous project restore worker.
pub(crate) struct ProjectRestoreExecutionOptions {
    pub(crate) operation: QueuedProjectRestore,
    pub(crate) shared: SharedInstancePlan,
    pub(crate) installation_id: String,
    pub(crate) network_name: String,
    pub(crate) schema_version: u32,
    pub(crate) state_database_path: PathBuf,
    pub(crate) backup_root: PathBuf,
    pub(crate) updated_at_unix_seconds: i64,
    pub(crate) timeout: Duration,
}
