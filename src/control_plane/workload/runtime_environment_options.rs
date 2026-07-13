use crate::control_plane::ProjectIdentity;
use crate::control_plane::state::ManagedEnvironmentRecord;
use std::collections::BTreeMap;

/// Complete declared and daemon-owned inputs for one project runtime.
pub(crate) struct RuntimeEnvironmentOptions {
    pub(crate) project: ProjectIdentity,
    pub(crate) declared: BTreeMap<String, String>,
    pub(crate) managed: ManagedEnvironmentRecord,
}
