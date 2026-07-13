use std::fmt::{Display, Formatter};

/// Durable credential or MySQL-family materialization failure.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct MySqlPreparationError {
    detail: String,
}

impl MySqlPreparationError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for MySqlPreparationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for MySqlPreparationError {}
