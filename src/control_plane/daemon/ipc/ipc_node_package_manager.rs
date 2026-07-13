use serde::{Deserialize, Serialize};

/// A known Node package manager transported without accepting an executable.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IpcNodePackageManager {
    Npm,
    Pnpm,
    Yarn,
}
