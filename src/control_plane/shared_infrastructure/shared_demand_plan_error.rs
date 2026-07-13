use std::fmt::{Display, Formatter};

/// Shared service demand that cannot safely join a compatibility group.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct SharedDemandPlanError {
    detail: String,
}

impl SharedDemandPlanError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for SharedDemandPlanError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for SharedDemandPlanError {}
