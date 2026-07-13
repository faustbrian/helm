use crate::control_plane::engine::ContainerCreateOptions;
use crate::control_plane::gateway::GatewaySnapshot;
use crate::control_plane::workload::ImmutableProjectApplicationPlan;

/// A complete, side-effect-free Engine pass derived before any mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EngineReconciliationPlan {
    applications: Vec<ImmutableProjectApplicationPlan>,
    dedicated_services: Vec<ContainerCreateOptions>,
    processes: Vec<ContainerCreateOptions>,
    gateway: GatewaySnapshot,
}

impl EngineReconciliationPlan {
    pub(super) const fn new(
        applications: Vec<ImmutableProjectApplicationPlan>,
        dedicated_services: Vec<ContainerCreateOptions>,
        processes: Vec<ContainerCreateOptions>,
        gateway: GatewaySnapshot,
    ) -> Self {
        Self {
            applications,
            dedicated_services,
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

    pub(crate) fn dedicated_services(&self) -> &[ContainerCreateOptions] {
        &self.dedicated_services
    }

    pub(crate) const fn gateway(&self) -> &GatewaySnapshot {
        &self.gateway
    }
}
