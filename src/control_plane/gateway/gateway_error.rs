use std::error::Error;
use std::fmt::{Display, Formatter};

/// A gateway plan validation or provider failure.
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum GatewayError {
    InvalidPlan { detail: String },
    Provider { detail: String },
}

impl Display for GatewayError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPlan { detail } | Self::Provider { detail } => formatter.write_str(detail),
        }
    }
}

impl Error for GatewayError {}
