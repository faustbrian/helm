use std::collections::BTreeMap;

use crate::control_plane::engine::ContainerHealth;

use super::{ResourceHealth, ResourceHealthRegistryError};

/// Non-durable health snapshot keyed by exact Engine resource identity.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ResourceHealthRegistry {
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

    pub(crate) fn observation(&self, resource_id: &str) -> Option<(ResourceHealth, i64)> {
        self.observations.get(resource_id).copied()
    }

    /// Invalidates every observation when the selected Engine adapter is lost.
    pub(crate) fn clear(&mut self) {
        self.observations.clear();
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

        Ok(())
    }
}
