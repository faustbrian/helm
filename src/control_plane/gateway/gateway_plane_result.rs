use super::{GatewayConfigurationAction, GatewayReconcileAction};
use crate::control_plane::engine::ContainerHealth;

/// Converged gateway container, readiness, and route-configuration outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GatewayPlaneResult {
    gateway_action: GatewayReconcileAction,
    health: ContainerHealth,
    configuration_action: GatewayConfigurationAction,
}

impl GatewayPlaneResult {
    pub(super) const fn new(
        gateway_action: GatewayReconcileAction,
        health: ContainerHealth,
        configuration_action: GatewayConfigurationAction,
    ) -> Self {
        Self {
            gateway_action,
            health,
            configuration_action,
        }
    }

    pub(crate) const fn gateway_action(&self) -> GatewayReconcileAction {
        self.gateway_action
    }

    pub(crate) const fn health(&self) -> ContainerHealth {
        self.health
    }

    pub(crate) const fn configuration_action(&self) -> GatewayConfigurationAction {
        self.configuration_action
    }
}
