use crate::control_plane::gateway::GatewaySnapshot;
use crate::control_plane::workload::{
    DedicatedProjectServicePlan, ImmutableProjectApplicationPlan, ProjectProcessOperationPlan,
    ScheduledProjectCommandPlan,
};

/// A complete, side-effect-free Engine pass derived before any mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EngineReconciliationPlan {
    applications: Vec<ImmutableProjectApplicationPlan>,
    dedicated_services: Vec<DedicatedProjectServicePlan>,
    processes: Vec<ProjectProcessOperationPlan>,
    scheduled_commands: Vec<ScheduledProjectCommandPlan>,
    gateway: GatewaySnapshot,
}

impl EngineReconciliationPlan {
    pub(super) const fn new(
        applications: Vec<ImmutableProjectApplicationPlan>,
        dedicated_services: Vec<DedicatedProjectServicePlan>,
        processes: Vec<ProjectProcessOperationPlan>,
        scheduled_commands: Vec<ScheduledProjectCommandPlan>,
        gateway: GatewaySnapshot,
    ) -> Self {
        Self {
            applications,
            dedicated_services,
            processes,
            scheduled_commands,
            gateway,
        }
    }

    pub(crate) fn applications(&self) -> &[ImmutableProjectApplicationPlan] {
        &self.applications
    }

    pub(crate) fn processes(&self) -> &[ProjectProcessOperationPlan] {
        &self.processes
    }

    pub(crate) fn scheduled_commands(&self) -> &[ScheduledProjectCommandPlan] {
        &self.scheduled_commands
    }

    pub(crate) fn dedicated_services(&self) -> &[DedicatedProjectServicePlan] {
        &self.dedicated_services
    }

    pub(crate) const fn gateway(&self) -> &GatewaySnapshot {
        &self.gateway
    }
}
