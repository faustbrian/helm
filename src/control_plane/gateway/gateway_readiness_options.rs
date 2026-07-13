use super::GatewayError;
use crate::control_plane::engine::OwnedContainer;
use std::time::Duration;

/// Bounded timing and identity inputs for one gateway readiness wait.
pub(crate) struct GatewayReadinessOptions<'operation> {
    gateway: &'operation OwnedContainer,
    timeout: Duration,
    poll_interval: Duration,
}

impl<'operation> GatewayReadinessOptions<'operation> {
    pub(crate) fn new(
        gateway: &'operation OwnedContainer,
        timeout: Duration,
        poll_interval: Duration,
    ) -> Result<Self, GatewayError> {
        Self::validate_timing(timeout, poll_interval)?;

        Ok(Self {
            gateway,
            timeout,
            poll_interval,
        })
    }

    pub(super) fn validate_timing(
        timeout: Duration,
        poll_interval: Duration,
    ) -> Result<(), GatewayError> {
        if timeout.is_zero() || poll_interval.is_zero() {
            return Err(GatewayError::InvalidPlan {
                detail: "gateway readiness timeout and poll interval must be nonzero".to_owned(),
            });
        }
        if poll_interval > timeout {
            return Err(GatewayError::InvalidPlan {
                detail: "gateway readiness poll interval must not exceed its timeout".to_owned(),
            });
        }

        Ok(())
    }

    pub(crate) const fn gateway(&self) -> &OwnedContainer {
        self.gateway
    }

    pub(crate) const fn timeout(&self) -> Duration {
        self.timeout
    }

    pub(crate) const fn poll_interval(&self) -> Duration {
        self.poll_interval
    }
}
