use super::MigrationDecisionExecutionResult;

/// One explicit migration decision owned by the singleton daemon.
pub(crate) struct ActiveMigrationDecision {
    operation_id: String,
    task: tokio::task::JoinHandle<MigrationDecisionExecutionResult>,
}

impl ActiveMigrationDecision {
    pub(crate) fn new(
        operation_id: String,
        task: tokio::task::JoinHandle<MigrationDecisionExecutionResult>,
    ) -> Self {
        Self { operation_id, task }
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.task.is_finished()
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        String,
        tokio::task::JoinHandle<MigrationDecisionExecutionResult>,
    ) {
        (self.operation_id, self.task)
    }
}
