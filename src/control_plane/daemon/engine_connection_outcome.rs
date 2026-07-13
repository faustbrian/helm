use super::RetryDelay;

/// Result of one non-blocking Engine availability poll.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum EngineConnectionOutcome {
    Connected,
    BackingOff { retry: RetryDelay },
    Unavailable { retry: RetryDelay, detail: String },
}
