use super::{ProjectRestoreTargetPlan, QueuedProjectRestore};
use std::path::PathBuf;
use std::time::Duration;

/// Complete owned context for one asynchronous project restore worker.
pub(crate) struct ProjectRestoreExecutionOptions {
    pub(crate) operation: QueuedProjectRestore,
    pub(crate) target: ProjectRestoreTargetPlan,
    pub(crate) installation_id: String,
    pub(crate) network_name: String,
    pub(crate) schema_version: u32,
    pub(crate) state_database_path: PathBuf,
    pub(crate) backup_root: PathBuf,
    pub(crate) updated_at_unix_seconds: i64,
    pub(crate) timeout: Duration,
}

impl ProjectRestoreExecutionOptions {
    pub(crate) fn shared_target(
        &self,
    ) -> Result<&crate::control_plane::shared_infrastructure::SharedInstancePlan, String> {
        self.target
            .shared()
            .ok_or_else(|| "project restore requires an exact shared target plan".to_owned())
    }

    pub(crate) fn dedicated_target(
        &self,
    ) -> Result<&crate::control_plane::workload::DedicatedProjectServicePlan, String> {
        self.target
            .dedicated()
            .ok_or_else(|| "project restore requires an exact dedicated target plan".to_owned())
    }
}
