use super::GlobalNetworkReconcileAction;
use crate::control_plane::engine::OwnedNetwork;

/// Proven global network and mutation resulting from one reconciliation pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GlobalNetworkReconcileResult {
    network: OwnedNetwork,
    action: GlobalNetworkReconcileAction,
}

impl GlobalNetworkReconcileResult {
    pub(super) const fn new(network: OwnedNetwork, action: GlobalNetworkReconcileAction) -> Self {
        Self { network, action }
    }

    pub(crate) const fn network(&self) -> &OwnedNetwork {
        &self.network
    }

    pub(crate) const fn action(&self) -> GlobalNetworkReconcileAction {
        self.action
    }
}
