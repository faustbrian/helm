use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct RabbitMqPlanError {
    detail: String,
}

impl RabbitMqPlanError {
    pub(crate) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for RabbitMqPlanError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl Error for RabbitMqPlanError {}
