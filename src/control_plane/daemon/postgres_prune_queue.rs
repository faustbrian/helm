use super::{PostgresPruneQueueError, QueuedPostgresPrune};
use std::collections::VecDeque;

const DEFAULT_CAPACITY: usize = 64;

/// Bounded FIFO serialized with every other singleton Engine mutation.
pub(crate) struct PostgresPruneQueue {
    capacity: usize,
    pending: VecDeque<QueuedPostgresPrune>,
}

impl PostgresPruneQueue {
    pub(crate) fn enqueue(
        &mut self,
        operation: QueuedPostgresPrune,
    ) -> Result<(), PostgresPruneQueueError> {
        if self
            .pending
            .iter()
            .any(|queued| queued.operation_id() == operation.operation_id())
        {
            return Err(PostgresPruneQueueError::DuplicateOperation {
                operation_id: operation.operation_id().to_owned(),
            });
        }
        if self.pending.len() == self.capacity {
            return Err(PostgresPruneQueueError::CapacityReached {
                capacity: self.capacity,
            });
        }
        self.pending.push_back(operation);

        Ok(())
    }

    pub(crate) fn pop_front(&mut self) -> Option<QueuedPostgresPrune> {
        self.pending.pop_front()
    }
    pub(crate) fn requeue_front(&mut self, operation: QueuedPostgresPrune) {
        self.pending.push_front(operation);
    }
    pub(crate) fn contains(&self, operation_id: &str) -> bool {
        self.pending
            .iter()
            .any(|operation| operation.operation_id() == operation_id)
    }
    pub(crate) fn remove(&mut self, operation_id: &str) -> Option<QueuedPostgresPrune> {
        let index = self
            .pending
            .iter()
            .position(|item| item.operation_id() == operation_id)?;
        self.pending.remove(index)
    }
    pub(crate) fn len(&self) -> usize {
        self.pending.len()
    }
}

impl Default for PostgresPruneQueue {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_CAPACITY,
            pending: VecDeque::with_capacity(DEFAULT_CAPACITY),
        }
    }
}
