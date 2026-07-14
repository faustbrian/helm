use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::state::StateStoreError;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Fail-closed preparation error retaining the last durable checkpoint.
#[derive(Debug)]
pub(crate) enum V7MigrationExecutionError {
    InvalidPlan {
        detail: String,
    },
    MissingAdapter {
        adapter_kind: String,
    },
    CheckpointMismatch,
    State(StateStoreError),
    Operation {
        adapter_id: String,
        action: &'static str,
        source: MigrationOperationError,
    },
}

impl Display for V7MigrationExecutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPlan { detail } => {
                write!(formatter, "invalid v7 execution plan: {detail}")
            }
            Self::MissingAdapter { adapter_kind } => {
                write!(
                    formatter,
                    "v7 migration adapter '{adapter_kind}' is unavailable"
                )
            }
            Self::CheckpointMismatch => {
                formatter.write_str("durable v7 execution does not match the selected plan")
            }
            Self::State(source) => write!(formatter, "v7 migration state failed: {source}"),
            Self::Operation {
                adapter_id,
                action,
                source,
            } => write!(
                formatter,
                "v7 migration adapter '{adapter_id}' {action} failed: {source}"
            ),
        }
    }
}

impl Error for V7MigrationExecutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::State(source) => Some(source),
            Self::Operation { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<StateStoreError> for V7MigrationExecutionError {
    fn from(source: StateStoreError) -> Self {
        Self::State(source)
    }
}
