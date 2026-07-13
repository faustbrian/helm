use std::error::Error;
use std::fmt::{Display, Formatter};

/// Invalid SQL Server compatibility or logical-resource input.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct SqlServerPlanError {
    detail: String,
}

impl SqlServerPlanError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for SqlServerPlanError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl Error for SqlServerPlanError {}
