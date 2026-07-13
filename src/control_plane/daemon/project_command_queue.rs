use super::{ProjectCommandQueueError, QueuedProjectCommand};
use std::collections::VecDeque;

const DEFAULT_PROJECT_COMMAND_CAPACITY: usize = 64;

/// Bounded FIFO for commands serialized through the singleton daemon.
pub(crate) struct ProjectCommandQueue {
    capacity: usize,
    pending: VecDeque<QueuedProjectCommand>,
}

impl ProjectCommandQueue {
    pub(crate) fn new(capacity: usize) -> Result<Self, ProjectCommandQueueError> {
        if capacity == 0 {
            return Err(ProjectCommandQueueError::InvalidCapacity);
        }

        Ok(Self {
            capacity,
            pending: VecDeque::with_capacity(capacity),
        })
    }

    pub(crate) fn enqueue(
        &mut self,
        operation: QueuedProjectCommand,
    ) -> Result<(), ProjectCommandQueueError> {
        if self
            .pending
            .iter()
            .any(|queued| queued.operation_id() == operation.operation_id())
        {
            return Err(ProjectCommandQueueError::DuplicateOperation {
                operation_id: operation.operation_id().to_owned(),
            });
        }
        if self.pending.len() == self.capacity {
            return Err(ProjectCommandQueueError::CapacityReached {
                capacity: self.capacity,
            });
        }
        self.pending.push_back(operation);

        Ok(())
    }

    pub(crate) fn pop_front(&mut self) -> Option<QueuedProjectCommand> {
        self.pending.pop_front()
    }

    pub(crate) fn front(&self) -> Option<&QueuedProjectCommand> {
        self.pending.front()
    }

    pub(crate) fn remove(&mut self, operation_id: &str) -> Option<QueuedProjectCommand> {
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

impl Default for ProjectCommandQueue {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_PROJECT_COMMAND_CAPACITY,
            pending: VecDeque::with_capacity(DEFAULT_PROJECT_COMMAND_CAPACITY),
        }
    }
}
