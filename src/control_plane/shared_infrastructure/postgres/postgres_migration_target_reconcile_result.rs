use super::PostgresSharedInstancePlan;
use crate::control_plane::engine::{ContainerHealth, OwnedContainer, OwnedVolume};
use crate::control_plane::state::CredentialRecord;
use crate::control_plane::workload::{ProjectVolumeReconcileResult, WorkloadReconcileResult};

/// Proven retained Engine resources and stable administrator for one target.
pub(crate) struct PostgresMigrationTargetReconcileResult {
    plan: PostgresSharedInstancePlan,
    service: WorkloadReconcileResult,
    volume: ProjectVolumeReconcileResult,
}

impl PostgresMigrationTargetReconcileResult {
    pub(super) const fn new(
        plan: PostgresSharedInstancePlan,
        service: WorkloadReconcileResult,
        volume: ProjectVolumeReconcileResult,
    ) -> Self {
        Self {
            plan,
            service,
            volume,
        }
    }

    pub(crate) const fn container(&self) -> &OwnedContainer {
        self.service.container()
    }

    pub(crate) const fn volume(&self) -> &OwnedVolume {
        self.volume.volume()
    }

    pub(crate) const fn health(&self) -> ContainerHealth {
        self.service.health()
    }

    pub(crate) const fn bootstrap_credential(&self) -> &CredentialRecord {
        self.plan.bootstrap_credential()
    }
}
