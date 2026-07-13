use super::ApplicationContainerPlan;
use crate::control_plane::engine::ManagedResourceMetadata;
use std::collections::BTreeMap;

/// Complete backend inputs for one dedicated application container mutation.
pub(crate) struct ApplicationContainerRequestOptions {
    pub(crate) plan: ApplicationContainerPlan,
    pub(crate) metadata: ManagedResourceMetadata,
    pub(crate) platform: String,
    pub(crate) command: Vec<String>,
    pub(crate) environment: BTreeMap<String, String>,
}
