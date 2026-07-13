use super::GatewayReconcileAction;
use crate::control_plane::engine::{ContainerHealth, OwnedContainer};

/// Owned gateway identity and observed state after reconciliation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GatewayReconcileResult {
    container: OwnedContainer,
    action: GatewayReconcileAction,
    health: ContainerHealth,
}

impl GatewayReconcileResult {
    pub(super) const fn new(
        container: OwnedContainer,
        action: GatewayReconcileAction,
        health: ContainerHealth,
    ) -> Self {
        Self {
            container,
            action,
            health,
        }
    }

    pub(crate) const fn container(&self) -> &OwnedContainer {
        &self.container
    }

    pub(crate) const fn action(&self) -> GatewayReconcileAction {
        self.action
    }

    pub(crate) const fn health(&self) -> ContainerHealth {
        self.health
    }
}
