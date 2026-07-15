use super::LogicalResourceDrift;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Ownership, validation, or Engine failure while converging shared resources.
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum SharedInfrastructureReconcileError {
    InvalidRequest { detail: String },
    Conflict { detail: String },
    LogicalResourceDrift { resource_id: String, detail: String },
    ProvisioningFailed { detail: String, status_code: i64 },
    Engine { action: String, detail: String },
}

impl SharedInfrastructureReconcileError {
    pub(crate) fn into_logical_resource_drift(self) -> Result<LogicalResourceDrift, Self> {
        match self {
            Self::LogicalResourceDrift {
                resource_id,
                detail,
            } => Ok(LogicalResourceDrift::new(resource_id, detail)),
            error => Err(error),
        }
    }
}

impl Display for SharedInfrastructureReconcileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest { detail }
            | Self::Conflict { detail }
            | Self::LogicalResourceDrift { detail, .. }
            | Self::ProvisioningFailed { detail, .. } => formatter.write_str(detail),
            Self::Engine { action, detail } => {
                write!(formatter, "shared infrastructure {action} failed: {detail}")
            }
        }
    }
}

impl Error for SharedInfrastructureReconcileError {}
