use super::ProjectCommandExecutionResult;

/// One owned project command task currently driven by the singleton daemon.
pub(crate) struct ActiveProjectCommand {
    operation_id: String,
    task: tokio::task::JoinHandle<ProjectCommandExecutionResult>,
}

impl ActiveProjectCommand {
    pub(crate) fn new(
        operation_id: String,
        task: tokio::task::JoinHandle<ProjectCommandExecutionResult>,
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
        tokio::task::JoinHandle<ProjectCommandExecutionResult>,
    ) {
        (self.operation_id, self.task)
    }
}
