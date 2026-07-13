use std::fmt::{Display, Formatter};

/// Unsafe or inconsistent project runtime environment composition.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct RuntimeEnvironmentError {
    detail: String,
}

impl RuntimeEnvironmentError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for RuntimeEnvironmentError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for RuntimeEnvironmentError {}
