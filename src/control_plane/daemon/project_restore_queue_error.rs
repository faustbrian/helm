use std::fmt::{Display, Formatter};

/// Bounded restore-queue admission failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ProjectRestoreQueueError {
    CapacityReached { capacity: usize },
    DuplicateOperation { operation_id: String },
}

impl Display for ProjectRestoreQueueError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CapacityReached { capacity } => write!(
                formatter,
                "project restore queue capacity {capacity} has been reached"
            ),
            Self::DuplicateOperation { operation_id } => write!(
                formatter,
                "project restore operation '{operation_id}' is already queued"
            ),
        }
    }
}

impl std::error::Error for ProjectRestoreQueueError {}
