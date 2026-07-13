use serde::{Deserialize, Serialize};

/// A typed successful daemon operation result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub(crate) enum IpcResult {
    /// Confirms the daemon is responsive.
    Pong,
    /// Confirms an asynchronous operation was accepted.
    Accepted { operation_id: String },
    /// Reports one complete watched-root reconciliation attempt.
    Reconciled {
        project_count: usize,
        issue_count: usize,
        applied: bool,
    },
}
