use std::fmt::{Display, Formatter};

/// Explicit capacity, identity, and cursor failures for live log sessions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ProjectLogSessionRegistryError {
    InvalidCapacity,
    DuplicateSession { session_id: String },
    CapacityReached { capacity: usize },
    UnknownSession { session_id: String },
    Buffer { detail: String },
}

impl Display for ProjectLogSessionRegistryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCapacity => {
                formatter.write_str("project log session capacity must be positive")
            }
            Self::DuplicateSession { session_id } => {
                write!(
                    formatter,
                    "project log session '{session_id}' already exists"
                )
            }
            Self::CapacityReached { capacity } => write!(
                formatter,
                "project log session capacity of {capacity} has been reached"
            ),
            Self::UnknownSession { session_id } => {
                write!(
                    formatter,
                    "project log session '{session_id}' does not exist"
                )
            }
            Self::Buffer { detail } => formatter.write_str(detail),
        }
    }
}

impl std::error::Error for ProjectLogSessionRegistryError {}
