use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PostgresPruneQueueError {
    InvalidCapacity,
    CapacityReached { capacity: usize },
    DuplicateOperation { operation_id: String },
}

impl Display for PostgresPruneQueueError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCapacity => {
                write!(formatter, "PostgreSQL prune capacity must be positive")
            }
            Self::CapacityReached { capacity } => write!(
                formatter,
                "PostgreSQL prune queue reached its capacity of {capacity}"
            ),
            Self::DuplicateOperation { operation_id } => write!(
                formatter,
                "PostgreSQL prune '{operation_id}' is already queued"
            ),
        }
    }
}

impl Error for PostgresPruneQueueError {}
