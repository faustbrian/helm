use std::error::Error;
use std::fmt::{Display, Formatter};

/// A bounded event journal cannot preserve the requested ordering contract.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum IpcEventJournalError {
    #[cfg_attr(not(test), expect(dead_code, reason = "validated test constructor"))]
    InvalidCapacity,
    #[cfg_attr(not(test), expect(dead_code, reason = "validated test append path"))]
    InvalidOperationId,
    SequenceExhausted,
    CorruptPersistedEvent {
        sequence: u64,
        detail: String,
    },
    NonMonotonicPersistedEvent {
        previous: u64,
        next: u64,
    },
    CursorExpired {
        requested: u64,
        oldest: u64,
    },
    CursorAhead {
        requested: u64,
        latest: u64,
    },
}

impl Display for IpcEventJournalError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCapacity => formatter.write_str("event journal capacity must be positive"),
            Self::InvalidOperationId => formatter.write_str("event operation ID must not be empty"),
            Self::SequenceExhausted => formatter.write_str("event sequence space is exhausted"),
            Self::CorruptPersistedEvent { sequence, detail } => write!(
                formatter,
                "persisted daemon event {sequence} is invalid: {detail}"
            ),
            Self::NonMonotonicPersistedEvent { previous, next } => write!(
                formatter,
                "persisted daemon event sequence {next} does not follow {previous} monotonically"
            ),
            Self::CursorExpired { requested, oldest } => write!(
                formatter,
                "event cursor {requested} is no longer retained; oldest available sequence is {oldest}"
            ),
            Self::CursorAhead { requested, latest } => write!(
                formatter,
                "event cursor {requested} is ahead of latest sequence {latest}"
            ),
        }
    }
}

impl Error for IpcEventJournalError {}
