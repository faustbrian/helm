use std::fmt::{Display, Formatter};

/// Durable preparation or Engine convergence failure for one migration target.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct PostgresMigrationTargetReconcileError {
    detail: String,
}

impl PostgresMigrationTargetReconcileError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for PostgresMigrationTargetReconcileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for PostgresMigrationTargetReconcileError {}
