use super::{DiscoveryScanReason, DiscoverySchedulerOptions};
use std::time::Instant;

/// Deterministic scheduling state for filesystem debounce and fallback scans.
pub(crate) struct DiscoveryScheduler {
    options: DiscoverySchedulerOptions,
    initial_deadline: Option<Instant>,
    first_pending_event: Option<Instant>,
    event_deadline: Option<Instant>,
    periodic_deadline: Instant,
}

impl DiscoveryScheduler {
    pub(crate) fn new(now: Instant, options: DiscoverySchedulerOptions) -> Self {
        Self {
            options,
            initial_deadline: Some(now),
            first_pending_event: None,
            event_deadline: None,
            periodic_deadline: now,
        }
    }

    pub(crate) fn record_filesystem_event(&mut self, now: Instant) {
        let first = *self.first_pending_event.get_or_insert(now);
        let quiet_deadline = now + self.options.debounce();
        let settle_deadline = first + self.options.maximum_settle();
        self.event_deadline = Some(quiet_deadline.min(settle_deadline));
    }

    pub(crate) fn next_deadline(&self) -> Instant {
        self.initial_deadline
            .or(self.event_deadline)
            .unwrap_or(self.periodic_deadline)
    }

    pub(crate) fn take_due(&mut self, now: Instant) -> Option<DiscoveryScanReason> {
        if self
            .initial_deadline
            .is_some_and(|deadline| now >= deadline)
        {
            self.initial_deadline = None;
            self.complete_scan(now);
            return Some(DiscoveryScanReason::Initial);
        }
        if self.event_deadline.is_some_and(|deadline| now >= deadline) {
            self.complete_scan(now);
            return Some(DiscoveryScanReason::FilesystemEvents);
        }
        if self.first_pending_event.is_none() && now >= self.periodic_deadline {
            self.complete_scan(now);
            return Some(DiscoveryScanReason::Periodic);
        }

        None
    }

    fn complete_scan(&mut self, now: Instant) {
        self.first_pending_event = None;
        self.event_deadline = None;
        self.periodic_deadline = now + self.options.periodic_rescan();
    }
}
