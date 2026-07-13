use std::fmt::{Display, Formatter};

/// Stateless Gotenberg materialization failure.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct GotenbergPreparationError {
    detail: String,
}

impl GotenbergPreparationError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for GotenbergPreparationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for GotenbergPreparationError {}
