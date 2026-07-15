use std::error::Error;
use std::fmt::{Display, Formatter};

/// Ownership, validation, or Engine failure while converging shared resources.
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum SharedInfrastructureReconcileError {
    InvalidRequest { detail: String },
    Conflict { detail: String },
    ProvisioningFailed { detail: String },
    Engine { action: String, detail: String },
}

impl Display for SharedInfrastructureReconcileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest { detail }
            | Self::Conflict { detail }
            | Self::ProvisioningFailed { detail } => formatter.write_str(detail),
            Self::Engine { action, detail } => {
                write!(formatter, "shared infrastructure {action} failed: {detail}")
            }
        }
    }
}

impl Error for SharedInfrastructureReconcileError {}
