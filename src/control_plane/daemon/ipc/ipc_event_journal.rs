use super::{IpcEvent, IpcEventJournalError, IpcEventKind};
use crate::control_plane::state::DaemonEventRecord;
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

    /// Restores a bounded in-memory cursor view from authoritative state.
    pub(crate) fn restore(records: Vec<DaemonEventRecord>) -> Result<Self, IpcEventJournalError> {
        let mut journal = Self::default();
        if records.len() > journal.capacity {
            return Err(IpcEventJournalError::CorruptPersistedEvent {
                sequence: records.first().map_or(0, DaemonEventRecord::sequence),
                detail: format!(
                    "{} retained events exceed capacity {}",
                    records.len(),
                    journal.capacity
                ),
            });
        }
        for record in records {
            journal.append_record(record)?;
        }

        Ok(journal)
    }

    pub(crate) const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Adds one event whose sequence was assigned transactionally by SQLite.
    pub(crate) fn append_record(
        &mut self,
        record: DaemonEventRecord,
    ) -> Result<IpcEvent, IpcEventJournalError> {
        let sequence = record.sequence();
        let previous = self.latest_sequence();
        if sequence == 0 || (!self.events.is_empty() && sequence <= previous) {
            return Err(IpcEventJournalError::NonMonotonicPersistedEvent {
                previous,
                next: sequence,
            });
        }
        let following = sequence
            .checked_add(1)
            .ok_or(IpcEventJournalError::SequenceExhausted)?;
        let event = IpcEvent::from_record(record).map_err(|error| {
            IpcEventJournalError::CorruptPersistedEvent {
                sequence,
                detail: error.to_string(),
            }
        })?;
        if event.operation_id().is_empty() {
            return Err(IpcEventJournalError::CorruptPersistedEvent {
                sequence,
                detail: "operation ID must not be empty".to_owned(),
            });
        }
        self.next_sequence = following;
        self.events.push_back(event.clone());
        if self.events.len() > self.capacity {
            drop(self.events.pop_front());
        }

        Ok(event)
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
