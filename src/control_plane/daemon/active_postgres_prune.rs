use super::PostgresPruneExecutionResult;
use tokio::task::JoinHandle;

/// One in-flight PostgreSQL prune owned by the singleton runtime.
pub(crate) struct ActivePostgresPrune {
    operation_id: String,
    task: JoinHandle<PostgresPruneExecutionResult>,
}

impl ActivePostgresPrune {
    pub(crate) const fn new(
        operation_id: String,
        task: JoinHandle<PostgresPruneExecutionResult>,
    ) -> Self {
        Self { operation_id, task }
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.task.is_finished()
    }

    pub(crate) fn into_parts(self) -> (String, JoinHandle<PostgresPruneExecutionResult>) {
        (self.operation_id, self.task)
    }
}
