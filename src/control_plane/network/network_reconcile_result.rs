use super::NetworkReconcileAction;
use crate::control_plane::engine::OwnedNetwork;

/// Proven managed network and mutation resulting from one reconciliation pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NetworkReconcileResult {
    network: OwnedNetwork,
    action: NetworkReconcileAction,
}

impl NetworkReconcileResult {
    pub(super) const fn new(network: OwnedNetwork, action: NetworkReconcileAction) -> Self {
        Self { network, action }
    }

    pub(crate) const fn network(&self) -> &OwnedNetwork {
        &self.network
    }

    pub(crate) const fn action(&self) -> NetworkReconcileAction {
        self.action
    }
}
