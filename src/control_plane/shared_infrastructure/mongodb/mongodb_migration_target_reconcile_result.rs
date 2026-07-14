use super::MongoDbSharedInstancePlan;
use crate::control_plane::engine::{OwnedContainer, OwnedVolume};
use crate::control_plane::state::CredentialRecord;
use crate::control_plane::workload::{ProjectVolumeReconcileResult, WorkloadReconcileResult};
use std::path::Path;

/// Proven retained Engine resources and stable administrator for one target.
pub(crate) struct MongoDbMigrationTargetReconcileResult {
    plan: MongoDbSharedInstancePlan,
    service: WorkloadReconcileResult,
    volume: ProjectVolumeReconcileResult,
}

impl MongoDbMigrationTargetReconcileResult {
    pub(super) const fn new(
        plan: MongoDbSharedInstancePlan,
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

    pub(crate) const fn bootstrap_credential(&self) -> &CredentialRecord {
        self.plan.bootstrap_credential()
    }

    pub(crate) fn bootstrap_secret_file(&self) -> &Path {
        self.plan.bootstrap_secret_file()
    }

    pub(crate) const fn plan(&self) -> &MongoDbSharedInstancePlan {
        &self.plan
    }
}
