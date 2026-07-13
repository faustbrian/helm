use std::fmt::{Display, Formatter};

/// Unsupported strategy or backend-specific durable preparation failure.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct SharedPreparationError {
    detail: String,
}

impl SharedPreparationError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for SharedPreparationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for SharedPreparationError {}
