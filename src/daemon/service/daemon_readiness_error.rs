use std::fmt::{Display, Formatter};

/// Classified readiness failure used to stop retrying terminal daemon state.
#[derive(Debug)]
pub(super) struct DaemonReadinessError {
    message: String,
    retryable: bool,
}

impl DaemonReadinessError {
    pub(super) fn new(message: impl Into<String>, retryable: bool) -> Self {
        Self {
            message: message.into(),
            retryable,
        }
    }

    pub(super) const fn retryable(&self) -> bool {
        self.retryable
    }
}

impl Display for DaemonReadinessError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for DaemonReadinessError {}
