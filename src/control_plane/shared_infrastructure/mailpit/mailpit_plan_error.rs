use std::error::Error;
use std::fmt::{Display, Formatter};

/// Invalid Mailpit attribution, persistence, or Engine planning input.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct MailpitPlanError {
    detail: String,
}

impl MailpitPlanError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for MailpitPlanError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl Error for MailpitPlanError {}
