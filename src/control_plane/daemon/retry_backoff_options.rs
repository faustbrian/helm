use super::RetryBackoffError;
use std::time::Duration;

const MAXIMUM_BACKOFF_DURATION: Duration = Duration::from_secs(24 * 60 * 60);

/// Validated bounds for one retry sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RetryBackoffOptions {
    initial: Duration,
    maximum: Duration,
}

impl RetryBackoffOptions {
    pub(crate) fn new(initial: Duration, maximum: Duration) -> Result<Self, RetryBackoffError> {
        if initial.is_zero() || maximum.is_zero() {
            return Err(RetryBackoffError::new(
                "retry backoff durations must be greater than zero",
            ));
        }
        if initial > maximum {
            return Err(RetryBackoffError::new(
                "retry initial duration must not exceed its maximum",
            ));
        }
        if maximum > MAXIMUM_BACKOFF_DURATION {
            return Err(RetryBackoffError::new(
                "retry maximum duration must not exceed 24 hours",
            ));
        }

        Ok(Self { initial, maximum })
    }

    pub(super) const fn initial(self) -> Duration {
        self.initial
    }

    pub(super) const fn maximum(self) -> Duration {
        self.maximum
    }
}
