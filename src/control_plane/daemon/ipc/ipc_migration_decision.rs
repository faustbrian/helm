use serde::{Deserialize, Serialize};

/// Explicit operator choice for one reversible migration checkpoint.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IpcMigrationDecision {
    Confirm,
    Rollback,
}

impl IpcMigrationDecision {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Confirm => "confirm",
            Self::Rollback => "rollback",
        }
    }
}
