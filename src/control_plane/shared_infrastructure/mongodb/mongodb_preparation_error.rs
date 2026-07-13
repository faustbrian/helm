use std::fmt::{Display, Formatter};

/// Durable credential or MongoDB materialization failure.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct MongoDbPreparationError {
    detail: String,
}

impl MongoDbPreparationError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for MongoDbPreparationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for MongoDbPreparationError {}
