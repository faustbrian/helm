use super::{EnvironmentLifecycle, ManagedEnvironmentRecordOptions};
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};

/// Complete daemon-owned environment injected into one project runtime.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct ManagedEnvironmentRecord {
    options: ManagedEnvironmentRecordOptions,
}

impl ManagedEnvironmentRecord {
    pub(crate) const fn new(options: ManagedEnvironmentRecordOptions) -> Self {
        Self { options }
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.options.project_id
    }

    pub(crate) fn revision(&self) -> &str {
        &self.options.revision
    }

    pub(crate) const fn values(&self) -> &BTreeMap<String, String> {
        &self.options.values
    }

    pub(crate) const fn lifecycle(&self) -> EnvironmentLifecycle {
        self.options.lifecycle
    }
}

impl Debug for ManagedEnvironmentRecord {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ManagedEnvironmentRecord")
            .field("project_id", &self.project_id())
            .field("revision", &self.revision())
            .field("value_keys", &self.values().keys())
            .field("lifecycle", &self.lifecycle())
            .finish()
    }
}
