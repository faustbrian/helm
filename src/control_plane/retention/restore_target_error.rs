use std::error::Error;
use std::fmt::{Display, Formatter};

/// A value-safe failure reported by a resource-specific restore adapter.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct RestoreTargetError {
    detail: String,
}

impl RestoreTargetError {
    pub(crate) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for RestoreTargetError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl Error for RestoreTargetError {}
