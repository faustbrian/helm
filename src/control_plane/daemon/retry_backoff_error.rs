use std::fmt::{Display, Formatter};

/// Invalid retry identity or timing policy.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct RetryBackoffError {
    detail: &'static str,
}

impl RetryBackoffError {
    pub(super) const fn new(detail: &'static str) -> Self {
        Self { detail }
    }
}

impl Display for RetryBackoffError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.detail)
    }
}

impl std::error::Error for RetryBackoffError {}
