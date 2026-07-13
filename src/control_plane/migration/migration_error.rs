use super::MigrationOperationError;
use crate::control_plane::state::StateStoreError;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// A migration that cannot safely advance, confirm, or roll back.
#[derive(Debug)]
pub(crate) enum MigrationError {
    State(StateStoreError),
    InvalidInventory {
        detail: String,
    },
    MissingCheckpoint {
        migration_id: String,
    },
    RecoveryPointNotFound {
        recovery_point_id: String,
        project_id: String,
    },
    RecoveryPointMismatch {
        recovery_point_id: String,
        project_id: String,
        service_id: String,
        logical_resource_id: String,
    },
    CheckpointMismatch {
        migration_id: String,
    },
    InvalidCheckpoint {
        detail: String,
    },
    InvalidAction {
        migration_id: String,
        detail: String,
    },
    Operation {
        action: &'static str,
        source: MigrationOperationError,
    },
}

impl Display for MigrationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::State(error) => Display::fmt(error, formatter),
            Self::InvalidInventory { detail } => {
                write!(formatter, "invalid migration inventory: {detail}")
            }
            Self::MissingCheckpoint { migration_id } => {
                write!(
                    formatter,
                    "migration '{migration_id}' has not been inventoried"
                )
            }
            Self::RecoveryPointNotFound {
                recovery_point_id,
                project_id,
            } => write!(
                formatter,
                "recovery point '{recovery_point_id}' does not exist for project '{project_id}'"
            ),
            Self::RecoveryPointMismatch {
                recovery_point_id,
                project_id,
                service_id,
                logical_resource_id,
            } => write!(
                formatter,
                "recovery point '{recovery_point_id}' does not match project '{project_id}' service '{service_id}' logical resource '{logical_resource_id}'"
            ),
            Self::CheckpointMismatch { migration_id } => write!(
                formatter,
                "migration '{migration_id}' checkpoint does not match its inventory"
            ),
            Self::InvalidCheckpoint { detail } => {
                write!(formatter, "invalid migration checkpoint: {detail}")
            }
            Self::InvalidAction {
                migration_id,
                detail,
            } => write!(formatter, "migration '{migration_id}' {detail}"),
            Self::Operation { action, source } => {
                write!(formatter, "migration {action} failed: {source}")
            }
        }
    }
}

impl Error for MigrationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::State(error) => Some(error),
            Self::Operation { source, .. } => Some(source),
            Self::InvalidInventory { .. }
            | Self::MissingCheckpoint { .. }
            | Self::RecoveryPointNotFound { .. }
            | Self::RecoveryPointMismatch { .. }
            | Self::CheckpointMismatch { .. }
            | Self::InvalidCheckpoint { .. }
            | Self::InvalidAction { .. } => None,
        }
    }
}

impl From<StateStoreError> for MigrationError {
    fn from(error: StateStoreError) -> Self {
        Self::State(error)
    }
}
