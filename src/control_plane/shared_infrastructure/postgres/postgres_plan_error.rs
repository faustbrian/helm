use std::error::Error;
use std::fmt::{Display, Formatter};

/// Invalid PostgreSQL logical-resource identity or provisioning input.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct PostgresPlanError {
    detail: String,
}

impl PostgresPlanError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for PostgresPlanError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl Error for PostgresPlanError {}
