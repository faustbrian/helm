use std::fmt::{Display, Formatter};

/// Invalid project infrastructure plan or durable credential preparation.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ProjectServicePreparationError {
    detail: String,
}

impl ProjectServicePreparationError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for ProjectServicePreparationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for ProjectServicePreparationError {}
