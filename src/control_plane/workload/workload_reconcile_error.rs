use std::error::Error;
use std::fmt::{Display, Formatter};

/// Ownership, Engine, or desired-state failure while converging a workload.
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum WorkloadReconcileError {
    Conflict { detail: String },
    DestructiveReplacementRequired { detail: String },
    Engine { action: String, detail: String },
    InvalidRequest { detail: String },
}

impl Display for WorkloadReconcileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict { detail }
            | Self::DestructiveReplacementRequired { detail }
            | Self::InvalidRequest { detail } => formatter.write_str(detail),
            Self::Engine { action, detail } => {
                write!(formatter, "workload {action} failed: {detail}")
            }
        }
    }
}

impl Error for WorkloadReconcileError {}
