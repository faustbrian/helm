use super::{ProjectBackupQueueError, QueuedProjectBackup};
use std::collections::VecDeque;

const DEFAULT_PROJECT_BACKUP_CAPACITY: usize = 64;

/// Bounded FIFO of recovery points serialized through the singleton daemon.
pub(crate) struct ProjectBackupQueue {
    capacity: usize,
    pending: VecDeque<QueuedProjectBackup>,
}

impl ProjectBackupQueue {
    pub(crate) fn new(capacity: usize) -> Result<Self, ProjectBackupQueueError> {
        if capacity == 0 {
            return Err(ProjectBackupQueueError::InvalidCapacity);
        }

        Ok(Self {
            capacity,
            pending: VecDeque::with_capacity(capacity),
        })
    }

    pub(crate) fn enqueue(
        &mut self,
        operation: QueuedProjectBackup,
    ) -> Result<(), ProjectBackupQueueError> {
        if self
            .pending
            .iter()
            .any(|queued| queued.operation_id() == operation.operation_id())
        {
            return Err(ProjectBackupQueueError::DuplicateOperation {
                operation_id: operation.operation_id().to_owned(),
            });
        }
        if self.pending.len() == self.capacity {
            return Err(ProjectBackupQueueError::CapacityReached {
                capacity: self.capacity,
            });
        }
        self.pending.push_back(operation);

        Ok(())
    }

    pub(crate) fn pop_front(&mut self) -> Option<QueuedProjectBackup> {
        self.pending.pop_front()
    }

    pub(crate) fn requeue_front(&mut self, operation: QueuedProjectBackup) {
        self.pending.push_front(operation);
    }

    pub(crate) fn remove(&mut self, operation_id: &str) -> Option<QueuedProjectBackup> {
        let index = self
            .pending
            .iter()
            .position(|operation| operation.operation_id() == operation_id)?;

        self.pending.remove(index)
    }

    pub(crate) fn len(&self) -> usize {
        self.pending.len()
    }
}

impl Default for ProjectBackupQueue {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_PROJECT_BACKUP_CAPACITY,
            pending: VecDeque::with_capacity(DEFAULT_PROJECT_BACKUP_CAPACITY),
        }
    }
}
