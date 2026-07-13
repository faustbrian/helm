use std::error::Error;
use std::fmt::{Display, Formatter};

/// Invalid MySQL-family instance or logical-resource plan.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct MySqlPlanError {
    detail: String,
}

impl MySqlPlanError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for MySqlPlanError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl Error for MySqlPlanError {}
