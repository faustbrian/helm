use serde::{Deserialize, Serialize};

/// Observable lifecycle of one bounded in-memory project log session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum IpcLogSessionState {
    Starting,
    Streaming,
    Completed,
    Failed { code: String, message: String },
    Cancelled,
}

impl IpcLogSessionState {
    pub(crate) const fn terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed { .. } | Self::Cancelled
        )
    }
}
