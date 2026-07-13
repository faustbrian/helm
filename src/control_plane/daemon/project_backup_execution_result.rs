use super::QueuedProjectBackup;
use crate::control_plane::migration::MigrationBackup;

/// Terminal evidence returned by one asynchronous recovery-point task.
pub(crate) struct ProjectBackupExecutionResult {
    operation: QueuedProjectBackup,
    created_at_unix_seconds: i64,
    outcome: Result<MigrationBackup, String>,
}

impl ProjectBackupExecutionResult {
    pub(crate) const fn new(
        operation: QueuedProjectBackup,
        created_at_unix_seconds: i64,
        outcome: Result<MigrationBackup, String>,
    ) -> Self {
        Self {
            operation,
            created_at_unix_seconds,
            outcome,
        }
    }

    #[cfg(test)]
    pub(crate) const fn outcome(&self) -> &Result<MigrationBackup, String> {
        &self.outcome
    }

    pub(crate) fn into_parts(self) -> (QueuedProjectBackup, i64, Result<MigrationBackup, String>) {
        (self.operation, self.created_at_unix_seconds, self.outcome)
    }
}
