use std::error::Error;
use std::fmt::{Display, Formatter};

/// A gateway plan validation or provider failure.
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum GatewayError {
    Engine { action: String, detail: String },
    InvalidPlan { detail: String },
    Preflight { detail: String },
    Provider { detail: String },
    Reconciliation { detail: String },
}

impl Display for GatewayError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Engine { action, detail } => {
                write!(formatter, "gateway {action} failed: {detail}")
            }
            Self::InvalidPlan { detail }
            | Self::Preflight { detail }
            | Self::Provider { detail }
            | Self::Reconciliation { detail } => formatter.write_str(detail),
        }
    }
}

impl Error for GatewayError {}
