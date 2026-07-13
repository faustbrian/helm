use std::fmt::{Display, Formatter};

/// Durable credential or RabbitMQ materialization failure.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct RabbitMqPreparationError {
    detail: String,
}

impl RabbitMqPreparationError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for RabbitMqPreparationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for RabbitMqPreparationError {}
