use crate::control_plane::engine::ContainerCreateOptions;
use crate::control_plane::gateway::GatewaySnapshot;
use crate::control_plane::workload::ImmutableProjectApplicationPlan;

/// A complete, side-effect-free Engine pass derived before any mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EngineReconciliationPlan {
    applications: Vec<ImmutableProjectApplicationPlan>,
    processes: Vec<ContainerCreateOptions>,
    gateway: GatewaySnapshot,
}

impl EngineReconciliationPlan {
    pub(super) const fn new(
        applications: Vec<ImmutableProjectApplicationPlan>,
        processes: Vec<ContainerCreateOptions>,
        gateway: GatewaySnapshot,
    ) -> Self {
        Self {
            applications,
            processes,
            gateway,
        }
    }

    pub(crate) fn applications(&self) -> &[ImmutableProjectApplicationPlan] {
        &self.applications
    }

    pub(crate) fn processes(&self) -> &[ContainerCreateOptions] {
        &self.processes
    }

    pub(crate) const fn gateway(&self) -> &GatewaySnapshot {
        &self.gateway
    }
}
