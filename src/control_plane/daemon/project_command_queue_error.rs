use std::error::Error;
use std::fmt::{Display, Formatter};

/// A project command cannot safely enter the bounded singleton queue.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum ProjectCommandQueueError {
    InvalidCapacity,
    DuplicateOperation { operation_id: String },
    CapacityReached { capacity: usize },
}

impl Display for ProjectCommandQueueError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCapacity => {
                formatter.write_str("project command queue capacity must be positive")
            }
            Self::DuplicateOperation { operation_id } => write!(
                formatter,
                "project command operation '{operation_id}' is already queued"
            ),
            Self::CapacityReached { capacity } => write!(
                formatter,
                "project command queue reached its capacity of {capacity} operations"
            ),
        }
    }
}

impl Error for ProjectCommandQueueError {}
