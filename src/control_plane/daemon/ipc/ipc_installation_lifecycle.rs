use crate::control_plane::state::InstallationLifecycle;
use serde::{Deserialize, Serialize};

/// Stable wire lifecycle for one installation deletion workflow.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IpcInstallationLifecycle {
    Active,
    Deleting,
    Deleted,
}

impl From<InstallationLifecycle> for IpcInstallationLifecycle {
    fn from(lifecycle: InstallationLifecycle) -> Self {
        match lifecycle {
            InstallationLifecycle::Active => Self::Active,
            InstallationLifecycle::Deleting => Self::Deleting,
            InstallationLifecycle::Deleted => Self::Deleted,
        }
    }
}
