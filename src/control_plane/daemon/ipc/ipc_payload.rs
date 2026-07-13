use serde::{Deserialize, Serialize};

/// A typed operation sent to the v8 daemon.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub(crate) enum IpcPayload {
    /// Verifies daemon availability and protocol compatibility.
    Ping,
    /// Requests one complete reconciliation of every authoritative watched root.
    Reconcile,
    /// Cancels an active request or stream by request ID.
    Cancel { target_request_id: String },
    /// Starts or resumes the ordered daemon event stream.
    SubscribeEvents { after_sequence: Option<u64> },
}
