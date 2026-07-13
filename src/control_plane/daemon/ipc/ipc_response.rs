use super::{IPC_PROTOCOL_VERSION, IpcResult};
use serde::{Deserialize, Serialize};

/// A stable machine-readable diagnostic returned by the daemon.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcDiagnostic {
    code: String,
    message: String,
    retryable: bool,
}

/// The typed outcome of one correlated IPC request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub(crate) enum IpcOutcome {
    /// The request completed successfully.
    Success { result: IpcResult },
    /// The request failed with structured diagnostics.
    Failure { diagnostics: Vec<IpcDiagnostic> },
}

/// One strictly encoded response frame from the v8 daemon.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcResponse {
    protocol_version: u16,
    request_id: String,
    outcome: IpcOutcome,
}

impl IpcResponse {
    /// Creates a successful response using the current protocol version.
    pub(crate) fn success(request_id: impl Into<String>, result: IpcResult) -> Self {
        Self {
            protocol_version: IPC_PROTOCOL_VERSION,
            request_id: request_id.into(),
            outcome: IpcOutcome::Success { result },
        }
    }

    /// Returns the declared wire protocol version.
    pub(crate) const fn protocol_version(&self) -> u16 {
        self.protocol_version
    }

    /// Returns the request ID this response completes or updates.
    pub(crate) fn request_id(&self) -> &str {
        &self.request_id
    }
}
