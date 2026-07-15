use super::MySqlSharedInstancePlan;
use crate::control_plane::engine::{ContainerHealth, OwnedContainer, OwnedVolume};
use crate::control_plane::state::CredentialRecord;
use crate::control_plane::workload::{ProjectVolumeReconcileResult, WorkloadReconcileResult};

/// Proven retained Engine resources and stable administrator for one target.
pub(crate) struct MySqlMigrationTargetReconcileResult {
    plan: MySqlSharedInstancePlan,
    service: WorkloadReconcileResult,
    volume: ProjectVolumeReconcileResult,
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

    pub(crate) const fn health(&self) -> ContainerHealth {
        self.health
    }

    pub(crate) const fn bootstrap_credential(&self) -> &CredentialRecord {
        self.plan.bootstrap_credential()
    }

    pub(crate) const fn plan(&self) -> &MySqlSharedInstancePlan {
        &self.plan
    }
}
