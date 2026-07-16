use std::error::Error;
use std::fmt::{Display, Formatter};

/// Safe failure to establish one exact owned private network.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum NetworkReconcileError {
    InvalidRequest { detail: String },
    Conflict { detail: String },
    EngineUnavailable { action: String, detail: String },
    Mutation { action: String, detail: String },
}

impl Display for NetworkReconcileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest { detail } => {
                write!(formatter, "invalid managed network request: {detail}")
            }
            Self::Conflict { detail } => write!(formatter, "managed network conflict: {detail}"),
            Self::EngineUnavailable { action, detail } | Self::Mutation { action, detail } => {
                write!(formatter, "managed network {action} failed: {detail}")
            }
        }
    }
}

impl Error for NetworkReconcileError {}
