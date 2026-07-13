use std::error::Error;
use std::fmt::{Display, Formatter};

/// An application workload that cannot be safely mapped to the Engine.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct WorkloadPlanError {
    detail: String,
}

impl WorkloadPlanError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for WorkloadPlanError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl Error for WorkloadPlanError {}
