use std::fmt::{Display, Formatter};

/// Durable credential or Redis-compatible materialization failure.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct RedisPreparationError {
    detail: String,
}

impl RedisPreparationError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for RedisPreparationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for RedisPreparationError {}
