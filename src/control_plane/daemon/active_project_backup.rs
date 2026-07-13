use super::ProjectBackupExecutionResult;

/// One project recovery-point task currently owned by the singleton daemon.
pub(crate) struct ActiveProjectBackup {
    operation_id: String,
    task: tokio::task::JoinHandle<ProjectBackupExecutionResult>,
}

impl ActiveProjectBackup {
    pub(crate) fn new(
        operation_id: String,
        task: tokio::task::JoinHandle<ProjectBackupExecutionResult>,
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
        tokio::task::JoinHandle<ProjectBackupExecutionResult>,
    ) {
        (self.operation_id, self.task)
    }
}
