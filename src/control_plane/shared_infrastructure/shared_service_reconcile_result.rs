use super::{SharedServiceReconcileAction, SharedVolumeReconcileResult};
use crate::control_plane::engine::{ContainerHealth, OwnedContainer};

/// Owned shared process and optional retained volume after reconciliation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SharedServiceReconcileResult {
    container: OwnedContainer,
    volume: Option<SharedVolumeReconcileResult>,
    action: SharedServiceReconcileAction,
    health: ContainerHealth,
}

impl SharedServiceReconcileResult {
    pub(super) const fn new(
        container: OwnedContainer,
        volume: Option<SharedVolumeReconcileResult>,
        action: SharedServiceReconcileAction,
        health: ContainerHealth,
    ) -> Self {
        Self {
            container,
            volume,
            action,
            health,
        }
    }

    pub(crate) const fn container(&self) -> &OwnedContainer {
        &self.container
    }

    pub(crate) const fn volume(&self) -> Option<&SharedVolumeReconcileResult> {
        self.volume.as_ref()
    }

    #[cfg(test)]
    pub(crate) const fn action(&self) -> SharedServiceReconcileAction {
        self.action
    }

    pub(crate) const fn health(&self) -> ContainerHealth {
        self.health
    }
}
