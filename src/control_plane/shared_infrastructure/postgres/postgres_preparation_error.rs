use std::fmt::{Display, Formatter};

/// Durable credential or PostgreSQL materialization failure.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct PostgresPreparationError {
    detail: String,
}

impl PostgresPreparationError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for PostgresPreparationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for PostgresPreparationError {}
