use serde::{Deserialize, Serialize};

/// Backup/restore boundary exposed without backend-specific implementation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IpcDataLifecycle {
    None,
    LogicalResource,
    SharedInstance,
}

impl IpcDataLifecycle {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::LogicalResource => "logical_resource",
            Self::SharedInstance => "shared_instance",
        }
    }
}
