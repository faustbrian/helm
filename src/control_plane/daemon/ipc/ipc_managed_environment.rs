use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};

use serde::{Deserialize, Serialize};

/// An explicitly requested managed environment with redacted debug output.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcManagedEnvironment {
    project: String,
    revision: String,
    values: BTreeMap<String, String>,
}

impl IpcManagedEnvironment {
    pub(crate) const fn new(
        project: String,
        revision: String,
        values: BTreeMap<String, String>,
    ) -> Self {
        Self {
            project,
            revision,
            values,
        }
    }

    pub(crate) const fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }
}

impl Debug for IpcManagedEnvironment {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("IpcManagedEnvironment")
            .field("project", &self.project)
            .field("revision", &self.revision)
            .field("value_keys", &self.values.keys())
            .finish()
    }
}
