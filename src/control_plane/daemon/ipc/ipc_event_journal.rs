use super::{IpcEvent, IpcEventJournalError, IpcEventKind};
use std::collections::VecDeque;

const DEFAULT_EVENT_CAPACITY: usize = 256;

/// Bounded in-memory event history with monotonic resumable cursors.
pub(crate) struct IpcEventJournal {
    capacity: usize,
    next_sequence: u64,
    events: VecDeque<IpcEvent>,
}

impl IpcEventJournal {
    pub(crate) fn new(capacity: usize) -> Result<Self, IpcEventJournalError> {
        if capacity == 0 {
            return Err(IpcEventJournalError::InvalidCapacity);
        }

        Ok(Self {
            capacity,
            next_sequence: 1,
            events: VecDeque::with_capacity(capacity),
        })
    }

    pub(crate) fn append(
        &mut self,
        operation_id: impl Into<String>,
        kind: IpcEventKind,
    ) -> Result<IpcEvent, IpcEventJournalError> {
        let operation_id = operation_id.into();
        if operation_id.is_empty() {
            return Err(IpcEventJournalError::InvalidOperationId);
        }
        let following = self
            .next_sequence
            .checked_add(1)
            .ok_or(IpcEventJournalError::SequenceExhausted)?;
        let event = IpcEvent::new(self.next_sequence, operation_id, kind);
        self.next_sequence = following;
        self.events.push_back(event.clone());
        if self.events.len() > self.capacity {
            drop(self.events.pop_front());
        }

        Ok(event)
    }

    pub(crate) fn events_after(
        &self,
        after_sequence: Option<u64>,
    ) -> Result<Vec<IpcEvent>, IpcEventJournalError> {
        let Some(after_sequence) = after_sequence else {
            return Ok(self.events.iter().cloned().collect());
        };
        let latest = self.latest_sequence();
        if after_sequence > latest {
            return Err(IpcEventJournalError::CursorAhead {
                requested: after_sequence,
                latest,
            });
        }
        if let Some(oldest) = self.events.front().map(IpcEvent::sequence)
            && after_sequence.saturating_add(1) < oldest
        {
            return Err(IpcEventJournalError::CursorExpired {
                requested: after_sequence,
                oldest,
            });
        }

        Ok(self
            .events
            .iter()
            .filter(|event| event.sequence() > after_sequence)
            .cloned()
            .collect())
    }

    pub(crate) const fn latest_sequence(&self) -> u64 {
        self.next_sequence - 1
    }
}

impl Default for IpcEventJournal {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_EVENT_CAPACITY,
            next_sequence: 1,
            events: VecDeque::with_capacity(DEFAULT_EVENT_CAPACITY),
        }
    }
}
