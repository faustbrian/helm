use super::MySqlSharedInstancePlan;
use crate::control_plane::engine::{ContainerHealth, OwnedContainer, OwnedVolume};
use crate::control_plane::workload::{ProjectVolumeReconcileResult, WorkloadReconcileResult};

/// Proven retained Engine resources and stable administrator for one target.
pub(crate) struct MySqlMigrationTargetReconcileResult {
    plan: MySqlSharedInstancePlan,
    service: WorkloadReconcileResult,
    volume: ProjectVolumeReconcileResult,
    #[cfg_attr(not(test), expect(dead_code, reason = "retained migration proof"))]
    health: ContainerHealth,
}

impl MySqlMigrationTargetReconcileResult {
    pub(super) const fn new(
        plan: MySqlSharedInstancePlan,
        service: WorkloadReconcileResult,
        volume: ProjectVolumeReconcileResult,
        health: ContainerHealth,
    ) -> Self {
        Self {
            plan,
            service,
            volume,
            health,
        }
    }

    pub(crate) const fn container(&self) -> &OwnedContainer {
        self.service.container()
    }

    pub(crate) const fn volume(&self) -> &OwnedVolume {
        self.volume.volume()
    }

    #[cfg(test)]
    pub(crate) const fn health(&self) -> ContainerHealth {
        self.health
    }

    pub(crate) const fn plan(&self) -> &MySqlSharedInstancePlan {
        &self.plan
    }
}
