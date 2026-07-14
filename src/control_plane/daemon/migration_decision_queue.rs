use super::{MigrationDecisionQueueError, QueuedMigrationDecision};
use std::collections::VecDeque;

const DEFAULT_MIGRATION_DECISION_CAPACITY: usize = 64;

/// Bounded FIFO of explicit confirm or rollback decisions.
pub(crate) struct MigrationDecisionQueue {
    capacity: usize,
    pending: VecDeque<QueuedMigrationDecision>,
}

impl MigrationDecisionQueue {
    pub(crate) fn new(capacity: usize) -> Result<Self, MigrationDecisionQueueError> {
        if capacity == 0 {
            return Err(MigrationDecisionQueueError::InvalidCapacity);
        }

        Ok(Self {
            capacity,
            pending: VecDeque::with_capacity(capacity),
        })
    }

    pub(crate) fn enqueue(
        &mut self,
        decision: QueuedMigrationDecision,
    ) -> Result<(), MigrationDecisionQueueError> {
        if self
            .pending
            .iter()
            .any(|queued| queued.operation_id() == decision.operation_id())
        {
            return Err(MigrationDecisionQueueError::DuplicateOperation {
                operation_id: decision.operation_id().to_owned(),
            });
        }
        if self.pending.len() == self.capacity {
            return Err(MigrationDecisionQueueError::CapacityReached {
                capacity: self.capacity,
            });
        }
        self.pending.push_back(decision);

        Ok(())
    }

    pub(crate) fn pop_front(&mut self) -> Option<QueuedMigrationDecision> {
        self.pending.pop_front()
    }

    pub(crate) fn requeue_front(&mut self, operation: QueuedMigrationDecision) {
        self.pending.push_front(operation);
    }

    pub(crate) fn front(&self) -> Option<&QueuedMigrationDecision> {
        self.pending.front()
    }

    pub(crate) fn remove(&mut self, operation_id: &str) -> Option<QueuedMigrationDecision> {
        let index = self
            .pending
            .iter()
            .position(|decision| decision.operation_id() == operation_id)?;

        self.pending.remove(index)
    }

    pub(crate) fn len(&self) -> usize {
        self.pending.len()
    }
}

impl Default for MigrationDecisionQueue {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_MIGRATION_DECISION_CAPACITY,
            pending: VecDeque::with_capacity(DEFAULT_MIGRATION_DECISION_CAPACITY),
        }
    }
}
