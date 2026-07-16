use super::QueuedProjectRestore;
use crate::control_plane::migration::MigrationExecutionResult;

/// Durable restore identity and its worker outcome.
pub(crate) struct ProjectRestoreExecutionResult {
    operation: QueuedProjectRestore,
    outcome: Result<MigrationExecutionResult, String>,
}

impl ProjectRestoreExecutionResult {
    pub(super) const fn new(
        operation: QueuedProjectRestore,
        outcome: Result<MigrationExecutionResult, String>,
    ) -> Self {
        Self { operation, outcome }
    }

    #[cfg(test)]
    pub(crate) const fn outcome(&self) -> &Result<MigrationExecutionResult, String> {
        &self.outcome
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        QueuedProjectRestore,
        Result<MigrationExecutionResult, String>,
    ) {
        (self.operation, self.outcome)
    }
}
