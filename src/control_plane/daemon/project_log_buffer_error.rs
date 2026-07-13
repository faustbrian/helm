use std::fmt::{Display, Formatter};

/// Explicit bounded-buffer and cursor failures for project log polling.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ProjectLogBufferError {
    InvalidCapacity,
    InvalidPageSize,
    CursorExpired { requested: u64, oldest: u64 },
    CursorAhead { requested: u64, latest: u64 },
}

impl Display for ProjectLogBufferError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCapacity => {
                formatter.write_str("project log buffer capacity must be positive")
            }
            Self::InvalidPageSize => formatter.write_str("project log page size must be positive"),
            Self::CursorExpired { requested, oldest } => write!(
                formatter,
                "project log cursor {requested} expired; oldest retained sequence is {oldest}"
            ),
            Self::CursorAhead { requested, latest } => write!(
                formatter,
                "project log cursor {requested} is ahead of latest sequence {latest}"
            ),
        }
    }
}

impl std::error::Error for ProjectLogBufferError {}
