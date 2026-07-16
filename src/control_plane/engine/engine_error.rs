use std::error::Error;
use std::fmt::{Display, Formatter};

use super::AttachedCommandOutput;

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
    ContainerExit {
        container_id: String,
        status_code: i64,
        output: Option<Box<AttachedCommandOutput>>,
    },
    OwnershipMismatch {
        action: &'static str,
        resource_kind: &'static str,
        resource_id: String,
    },
    Timeout {
        action: String,
        timeout_milliseconds: u64,
    },
}

impl EngineError {
    #[cfg(test)]
    pub(crate) fn attached_command_output(&self) -> Option<&AttachedCommandOutput> {
        match self {
            Self::ContainerExit {
                output: Some(output),
                ..
            } => Some(output.as_ref()),
            _ => None,
        }
    }

    pub(crate) fn take_attached_command_output(&mut self) -> Option<AttachedCommandOutput> {
        match self {
            Self::ContainerExit { output, .. } => output.take().map(|output| *output),
            _ => None,
        }
    }
}

impl Display for EngineError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest { detail } | Self::Backend { detail } => {
                formatter.write_str(detail)
            }
            Self::ContainerExit {
                container_id,
                status_code,
                ..
            } => write!(
                formatter,
                "container '{container_id}' exited with status {status_code}"
            ),
            Self::OwnershipMismatch {
                action,
                resource_kind,
                resource_id,
            } => write!(
                formatter,
                "refusing to {action} {resource_kind} '{resource_id}' because its Engine ownership labels no longer match"
            ),
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
