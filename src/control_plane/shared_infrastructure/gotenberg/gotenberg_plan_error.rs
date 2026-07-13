use std::error::Error;
use std::fmt::{Display, Formatter};

/// Invalid Gotenberg stateless-sharing input.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct GotenbergPlanError {
    detail: String,
}

impl GotenbergPlanError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for GotenbergPlanError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl Error for GotenbergPlanError {}
