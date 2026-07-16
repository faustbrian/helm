use std::collections::BTreeMap;

use crate::control_plane::engine::ContainerHealth;

use super::{ResourceHealth, ResourceHealthRegistryError};

/// Non-durable health snapshot keyed by exact Engine resource identity.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ResourceHealthRegistry {
    engine_unavailable: bool,
    discovery_complete: bool,
    engine_converged: bool,
    observations: BTreeMap<String, (ResourceHealth, i64)>,
}

impl ResourceHealthRegistry {
    pub(crate) fn record(
        &mut self,
        resource_id: &str,
        health: ContainerHealth,
        observed_at_unix_seconds: i64,
    ) -> Result<(), ResourceHealthRegistryError> {
        self.record_observation(resource_id, health.into(), observed_at_unix_seconds)
    }

    pub(crate) fn record_service_not_ready(
        &mut self,
        resource_id: &str,
        attempt: u32,
        observed_at_unix_seconds: i64,
    ) -> Result<(), ResourceHealthRegistryError> {
        self.record_observation(
            resource_id,
            ResourceHealth::ServiceNotReady { attempt },
            observed_at_unix_seconds,
        )
    }

    pub(crate) fn record_authentication_failed(
        &mut self,
        resource_id: &str,
        attempt: u32,
        observed_at_unix_seconds: i64,
    ) -> Result<(), ResourceHealthRegistryError> {
        self.record_observation(
            resource_id,
            ResourceHealth::AuthenticationFailed { attempt },
            observed_at_unix_seconds,
        )
    }

    pub(crate) fn record_destructive_replacement_required(
        &mut self,
        resource_id: &str,
        observed_at_unix_seconds: i64,
    ) -> Result<(), ResourceHealthRegistryError> {
        self.record_observation(
            resource_id,
            ResourceHealth::DestructiveReplacementRequired,
            observed_at_unix_seconds,
        )
    }

    pub(crate) fn record_logical_resource_drift(
        &mut self,
        resource_id: &str,
        observed_at_unix_seconds: i64,
    ) -> Result<(), ResourceHealthRegistryError> {
        self.record_observation(
            resource_id,
            ResourceHealth::LogicalResourceDrift,
            observed_at_unix_seconds,
        )
    }

    pub(crate) fn record_gateway_route_drift(
        &mut self,
        domain: &str,
        observed_at_unix_seconds: i64,
    ) -> Result<(), ResourceHealthRegistryError> {
        self.record_observation(
            domain,
            ResourceHealth::GatewayRouteDrift,
            observed_at_unix_seconds,
        )
    }

    pub(crate) fn record_certificate_expired(
        &mut self,
        domain: &str,
        observed_at_unix_seconds: i64,
    ) -> Result<(), ResourceHealthRegistryError> {
        self.record_observation(
            domain,
            ResourceHealth::CertificateExpired,
            observed_at_unix_seconds,
        )
    }

    pub(crate) fn observation(&self, resource_id: &str) -> Option<(ResourceHealth, i64)> {
        self.observations.get(resource_id).copied()
    }

    pub(crate) const fn engine_is_unavailable(&self) -> bool {
        self.engine_unavailable
    }

    pub(crate) const fn discovery_is_complete(&self) -> bool {
        self.discovery_complete
    }

    pub(crate) const fn engine_is_converged(&self) -> bool {
        self.engine_converged
    }

    pub(crate) fn record_operational_readiness(
        &mut self,
        discovery_complete: bool,
        engine_converged: bool,
    ) {
        self.discovery_complete = discovery_complete;
        self.engine_converged = engine_converged;
    }

    pub(crate) fn mark_engine_available(&mut self) {
        self.engine_unavailable = false;
    }

    pub(crate) fn mark_engine_unavailable(&mut self) {
        self.engine_unavailable = true;
        self.engine_converged = false;
        self.observations.clear();
    }

    /// Invalidates every observation when the selected Engine adapter is lost.
    pub(crate) fn clear(&mut self) {
        self.mark_engine_unavailable();
    }

    fn record_observation(
        &mut self,
        resource_id: &str,
        health: ResourceHealth,
        observed_at_unix_seconds: i64,
    ) -> Result<(), ResourceHealthRegistryError> {
        if resource_id.is_empty() {
            return Err(ResourceHealthRegistryError::EmptyResourceId);
        }
        if observed_at_unix_seconds < 0 {
            return Err(ResourceHealthRegistryError::NegativeObservationTime);
        }
        self.observations
            .insert(resource_id.to_owned(), (health, observed_at_unix_seconds));
        self.mark_engine_available();

        Ok(())
    }
}
