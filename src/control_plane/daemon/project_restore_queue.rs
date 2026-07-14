use super::{ProjectRestoreQueueError, QueuedProjectRestore};
use std::collections::VecDeque;

const DEFAULT_PROJECT_RESTORE_CAPACITY: usize = 64;

/// Bounded FIFO of exact recovery points serialized by the singleton daemon.
pub(crate) struct ProjectRestoreQueue {
    capacity: usize,
    pending: VecDeque<QueuedProjectRestore>,
}

impl ProjectRestoreQueue {
    pub(crate) fn new(capacity: usize) -> Result<Self, ProjectRestoreQueueError> {
        if capacity == 0 {
            return Err(ProjectRestoreQueueError::InvalidCapacity);
        }

        Ok(Self {
            capacity,
            pending: VecDeque::with_capacity(capacity),
        })
    }

    pub(crate) fn enqueue(
        &mut self,
        operation: QueuedProjectRestore,
    ) -> Result<(), ProjectRestoreQueueError> {
        if self
            .pending
            .iter()
            .any(|queued| queued.operation_id() == operation.operation_id())
        {
            return Err(ProjectRestoreQueueError::DuplicateOperation {
                operation_id: operation.operation_id().to_owned(),
            });
        }
        if self.pending.len() == self.capacity {
            return Err(ProjectRestoreQueueError::CapacityReached {
                capacity: self.capacity,
            });
        }
        self.pending.push_back(operation);

        Ok(())
    }

    pub(crate) fn pop_front(&mut self) -> Option<QueuedProjectRestore> {
        self.pending.pop_front()
    }

    pub(crate) fn requeue_front(&mut self, operation: QueuedProjectRestore) {
        self.pending.push_front(operation);
    }

    pub(crate) fn front(&self) -> Option<&QueuedProjectRestore> {
        self.pending.front()
    }

    pub(crate) fn remove(&mut self, operation_id: &str) -> Option<QueuedProjectRestore> {
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

impl Default for ProjectRestoreQueue {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_PROJECT_RESTORE_CAPACITY,
            pending: VecDeque::with_capacity(DEFAULT_PROJECT_RESTORE_CAPACITY),
        }
    }
}
