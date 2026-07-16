use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PostgresPruneQueueError {
    CapacityReached { capacity: usize },
    DuplicateOperation { operation_id: String },
}

impl Display for PostgresPruneQueueError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CapacityReached { capacity } => write!(
                formatter,
                "logical prune queue reached its capacity of {capacity}"
            ),
            Self::DuplicateOperation { operation_id } => write!(
                formatter,
                "logical prune '{operation_id}' is already queued"
            ),
        }
    }
}

impl Error for PostgresPruneQueueError {}
