use std::error::Error;
use std::fmt::{Display, Formatter};

/// A recovery-point intent cannot safely enter the singleton queue.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum ProjectBackupQueueError {
    #[cfg_attr(not(test), expect(dead_code, reason = "validated test constructor"))]
    InvalidCapacity,
    DuplicateOperation {
        operation_id: String,
    },
    CapacityReached {
        capacity: usize,
    },
}

impl Display for ProjectBackupQueueError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCapacity => {
                formatter.write_str("project backup queue capacity must be positive")
            }
            Self::DuplicateOperation { operation_id } => write!(
                formatter,
                "project backup operation '{operation_id}' is already queued"
            ),
            Self::CapacityReached { capacity } => write!(
                formatter,
                "project backup queue reached its capacity of {capacity} operations"
            ),
        }
    }
}

impl Error for ProjectBackupQueueError {}
