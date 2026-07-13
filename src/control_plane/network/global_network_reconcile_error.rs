use std::error::Error;
use std::fmt::{Display, Formatter};

/// Safe failure to establish exactly one owned global network.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum GlobalNetworkReconcileError {
    InvalidRequest { detail: String },
    Conflict { detail: String },
    EngineUnavailable { action: String, detail: String },
    Mutation { action: String, detail: String },
}

impl Display for GlobalNetworkReconcileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest { detail } => {
                write!(formatter, "invalid global network request: {detail}")
            }
            Self::Conflict { detail } => write!(formatter, "global network conflict: {detail}"),
            Self::EngineUnavailable { action, detail } | Self::Mutation { action, detail } => {
                write!(formatter, "global network {action} failed: {detail}")
            }
        }
    }
}

impl Error for GlobalNetworkReconcileError {}
