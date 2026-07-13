use super::EnvironmentLifecycle;
use std::collections::BTreeMap;

/// Complete fields for one daemon-owned project environment revision.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct ManagedEnvironmentRecordOptions {
    pub(crate) project_id: String,
    pub(crate) revision: String,
    pub(crate) values: BTreeMap<String, String>,
    pub(crate) lifecycle: EnvironmentLifecycle,
}
