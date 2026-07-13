use super::QueuedProjectCommand;
use crate::control_plane::engine::EngineError;
use crate::control_plane::workload::EphemeralBrowserPlan;

/// Complete inputs for one background project command execution task.
pub(crate) struct ProjectCommandExecutionOptions {
    pub(crate) operation: QueuedProjectCommand,
    pub(crate) installation_id: String,
    pub(crate) schema_version: u32,
    pub(crate) ephemeral_browser: Option<Result<EphemeralBrowserPlan, EngineError>>,
}
