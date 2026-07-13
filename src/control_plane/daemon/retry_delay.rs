use std::time::Duration;

/// One scheduled retry with its one-based consecutive failure count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RetryDelay {
    attempt: u32,
    duration: Duration,
}

impl RetryDelay {
    pub(super) const fn new(attempt: u32, duration: Duration) -> Self {
        Self { attempt, duration }
    }

    pub(crate) const fn attempt(self) -> u32 {
        self.attempt
    }

    pub(crate) const fn duration(self) -> Duration {
        self.duration
    }
}
