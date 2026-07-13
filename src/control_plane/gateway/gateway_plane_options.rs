use super::{GatewayError, GatewayReadinessOptions, GatewayReconcileOptions, GatewaySnapshot};
use std::time::Duration;

/// Complete inputs for reconciling the singleton gateway and its route state.
pub(crate) struct GatewayPlaneOptions<'operation> {
    pub(super) gateway: GatewayReconcileOptions<'operation>,
    pub(super) snapshot: &'operation GatewaySnapshot,
    pub(super) readiness_timeout: Duration,
    pub(super) readiness_poll_interval: Duration,
}

impl<'operation> GatewayPlaneOptions<'operation> {
    pub(crate) fn new(
        gateway: GatewayReconcileOptions<'operation>,
        snapshot: &'operation GatewaySnapshot,
        readiness_timeout: Duration,
        readiness_poll_interval: Duration,
    ) -> Result<Self, GatewayError> {
        GatewayReadinessOptions::validate_timing(readiness_timeout, readiness_poll_interval)?;

        Ok(Self {
            gateway,
            snapshot,
            readiness_timeout,
            readiness_poll_interval,
        })
    }
}
