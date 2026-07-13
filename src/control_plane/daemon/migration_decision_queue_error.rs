use std::fmt::{Display, Formatter};

/// Bounded migration-decision admission failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MigrationDecisionQueueError {
    InvalidCapacity,
    DuplicateOperation { operation_id: String },
    CapacityReached { capacity: usize },
}

impl Display for MigrationDecisionQueueError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCapacity => {
                write!(formatter, "migration decision capacity must be positive")
            }
            Self::DuplicateOperation { operation_id } => write!(
                formatter,
                "migration decision operation '{operation_id}' is already queued"
            ),
            Self::CapacityReached { capacity } => write!(
                formatter,
                "migration decision queue capacity {capacity} has been reached"
            ),
        }
    }
}

impl std::error::Error for MigrationDecisionQueueError {}
