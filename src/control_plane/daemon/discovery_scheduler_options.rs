use super::DiscoverySchedulerError;
use std::time::Duration;

/// Validated debounce, settle, and correctness-rescan timing policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DiscoverySchedulerOptions {
    debounce: Duration,
    maximum_settle: Duration,
    periodic_rescan: Duration,
}

impl DiscoverySchedulerOptions {
    pub(crate) fn new(
        debounce: Duration,
        maximum_settle: Duration,
        periodic_rescan: Duration,
    ) -> Result<Self, DiscoverySchedulerError> {
        if debounce.is_zero() || maximum_settle.is_zero() || periodic_rescan.is_zero() {
            return Err(DiscoverySchedulerError::new(
                "discovery scheduler durations must be greater than zero",
            ));
        }
        if maximum_settle < debounce {
            return Err(DiscoverySchedulerError::new(
                "discovery maximum settle duration must not be shorter than debounce",
            ));
        }

        Ok(Self {
            debounce,
            maximum_settle,
            periodic_rescan,
        })
    }

    pub(super) const fn debounce(self) -> Duration {
        self.debounce
    }

    pub(super) const fn maximum_settle(self) -> Duration {
        self.maximum_settle
    }

    pub(super) const fn periodic_rescan(self) -> Duration {
        self.periodic_rescan
    }
}
