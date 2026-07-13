use serde::{Deserialize, Serialize};

/// Durable lifecycle of one project-visible runtime or logical resource.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IpcResourceLifecycle {
    Active,
    Orphaned,
    Retained,
}

impl IpcResourceLifecycle {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Orphaned => "orphaned",
            Self::Retained => "retained",
        }
    }
}
