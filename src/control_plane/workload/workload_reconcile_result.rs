use super::WorkloadReconcileAction;
use crate::control_plane::engine::{ContainerHealth, OwnedContainer};

/// Owned workload identity and observed state after reconciliation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WorkloadReconcileResult {
    container: OwnedContainer,
    action: WorkloadReconcileAction,
    health: ContainerHealth,
}

impl WorkloadReconcileResult {
    pub(super) const fn new(
        container: OwnedContainer,
        action: WorkloadReconcileAction,
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

    pub(crate) const fn action(&self) -> WorkloadReconcileAction {
        self.action
    }

    pub(crate) const fn health(&self) -> ContainerHealth {
        self.health
    }
}
