use std::error::Error;
use std::fmt::{Display, Formatter};

/// A typed Engine request validation or backend failure.
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum EngineError {
    InvalidRequest {
        detail: String,
    },
    Backend {
        detail: String,
    },
    Timeout {
        action: String,
        timeout_milliseconds: u64,
    },
}

impl Display for EngineError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest { detail } | Self::Backend { detail } => {
                formatter.write_str(detail)
            }
            Self::Timeout {
                action,
                timeout_milliseconds,
            } => write!(
                formatter,
                "Engine operation '{action}' timed out after {timeout_milliseconds} ms"
            ),
        }
    }
}

impl Error for EngineError {}
