use std::fmt::{Display, Formatter};

/// A complete registry that cannot yet become safe Engine mutations.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct EngineReconciliationPlanError {
    detail: String,
}

impl EngineReconciliationPlanError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for EngineReconciliationPlanError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for EngineReconciliationPlanError {}
