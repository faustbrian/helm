use super::ProjectProcessPlan;
use crate::control_plane::engine::ManagedResourceMetadata;

/// Complete backend inputs for one supervised project process container.
pub(crate) struct ProjectProcessRequestOptions {
    pub(crate) plan: ProjectProcessPlan,
    pub(crate) metadata: ManagedResourceMetadata,
    pub(crate) platform: String,
}
