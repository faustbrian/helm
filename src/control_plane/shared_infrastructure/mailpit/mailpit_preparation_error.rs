use std::fmt::{Display, Formatter};

/// Durable credential or attributed Mailpit materialization failure.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct MailpitPreparationError {
    detail: String,
}

impl MailpitPreparationError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for MailpitPreparationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for MailpitPreparationError {}
