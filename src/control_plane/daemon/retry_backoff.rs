use super::{RetryBackoffError, RetryBackoffOptions, RetryDelay};
use sha2::{Digest, Sha256};

/// Per-resource bounded exponential retry state with stable equal jitter.
pub(crate) struct RetryBackoff {
    identity: String,
    options: RetryBackoffOptions,
    attempts: u32,
}

impl RetryBackoff {
    pub(crate) fn new(
        identity: impl Into<String>,
        options: RetryBackoffOptions,
    ) -> Result<Self, RetryBackoffError> {
        let identity = identity.into();
        if identity.is_empty() {
            return Err(RetryBackoffError::new(
                "retry backoff identity must not be empty",
            ));
        }

        Ok(Self {
            identity,
            options,
            attempts: 0,
        })
    }

    pub(crate) fn next_delay(&mut self) -> RetryDelay {
        let exponent = self.attempts.min(31);
        let multiplier = 1_u32 << exponent;
        let ceiling = self
            .options
            .initial()
            .saturating_mul(multiplier)
            .min(self.options.maximum());
        let duration = ceiling
            .mul_f64(0.5 + stable_fraction(&self.identity, self.attempts) * 0.5)
            .min(self.options.maximum());
        self.attempts = self.attempts.saturating_add(1);

        RetryDelay::new(self.attempts, duration)
    }

    pub(crate) const fn reset(&mut self) {
        self.attempts = 0;
    }
}

fn stable_fraction(identity: &str, attempt: u32) -> f64 {
    let mut digest = Sha256::new();
    digest.update(identity.as_bytes());
    digest.update(attempt.to_le_bytes());
    let hash: [u8; 32] = digest.finalize().into();
    let [a, b, c, d, e, f, g, h, ..] = hash;
    let value = u64::from_le_bytes([a, b, c, d, e, f, g, h]);

    value as f64 / u64::MAX as f64
}
