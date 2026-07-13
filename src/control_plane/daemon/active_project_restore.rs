use super::ProjectRestoreExecutionResult;

/// One retained recovery-point restore owned by the singleton daemon.
pub(crate) struct ActiveProjectRestore {
    operation_id: String,
    task: tokio::task::JoinHandle<ProjectRestoreExecutionResult>,
}

impl ActiveProjectRestore {
    pub(crate) fn new(
        operation_id: String,
        task: tokio::task::JoinHandle<ProjectRestoreExecutionResult>,
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
        tokio::task::JoinHandle<ProjectRestoreExecutionResult>,
    ) {
        (self.operation_id, self.task)
    }
}
