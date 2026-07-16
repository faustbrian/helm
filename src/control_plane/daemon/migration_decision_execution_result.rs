use super::QueuedMigrationDecision;
use crate::control_plane::migration::MigrationExecutionResult;

/// Durable decision identity and its worker outcome.
pub(crate) struct MigrationDecisionExecutionResult {
    operation: QueuedMigrationDecision,
    outcome: Result<MigrationExecutionResult, String>,
}

impl MigrationDecisionExecutionResult {
    pub(super) const fn new(
        operation: QueuedMigrationDecision,
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
        QueuedMigrationDecision,
        Result<MigrationExecutionResult, String>,
    ) {
        (self.operation, self.outcome)
    }
}
