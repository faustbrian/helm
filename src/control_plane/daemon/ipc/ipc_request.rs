use super::IpcPayload;
use serde::{Deserialize, Serialize};

/// The current local IPC protocol version.
pub(crate) const IPC_PROTOCOL_VERSION: u16 = 1;

/// One strictly decoded request frame sent to the v8 daemon.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcRequest {
    protocol_version: u16,
    request_id: String,
    payload: IpcPayload,
}

impl IpcRequest {
    /// Creates a request using the current protocol version.
    pub(crate) fn new(request_id: impl Into<String>, payload: IpcPayload) -> Self {
        Self {
            protocol_version: IPC_PROTOCOL_VERSION,
            request_id: request_id.into(),
            payload,
        }
    }

    /// Returns the declared wire protocol version.
    pub(crate) const fn protocol_version(&self) -> u16 {
        self.protocol_version
    }

    /// Returns the client-generated correlation and cancellation ID.
    pub(crate) fn request_id(&self) -> &str {
        &self.request_id
    }
}
