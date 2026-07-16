use std::error::Error;
use std::fmt::{Display, Formatter};

/// Fail-closed stale project network cleanup failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum StaleProjectNetworkCleanupError {
    Ownership { detail: String },
    Engine { action: String, detail: String },
}

impl Display for StaleProjectNetworkCleanupError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ownership { detail } => {
                write!(
                    formatter,
                    "stale project network ownership failed: {detail}"
                )
            }
            Self::Engine { action, detail } => {
                write!(formatter, "stale project network {action} failed: {detail}")
            }
        }
    }
}

impl Error for StaleProjectNetworkCleanupError {}
