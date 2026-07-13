use crate::control_plane::migration::MigrationBackup;

/// Terminal evidence returned by one asynchronous recovery-point task.
pub(crate) struct ProjectBackupExecutionResult {
    operation_id: String,
    outcome: Result<MigrationBackup, String>,
}

impl ProjectBackupExecutionResult {
    pub(crate) const fn new(
        operation_id: String,
        outcome: Result<MigrationBackup, String>,
    ) -> Self {
        Self {
            operation_id,
            outcome,
        }
    }

    #[cfg(test)]
    pub(crate) const fn outcome(&self) -> &Result<MigrationBackup, String> {
        &self.outcome
    }

    pub(crate) fn into_parts(self) -> (String, Result<MigrationBackup, String>) {
        (self.operation_id, self.outcome)
    }
}
