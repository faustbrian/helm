use super::RawServiceConfig;
use serde::Deserialize;
use std::collections::BTreeMap;

/// The strict serialized shape of one v8 `.stackctl.yaml` file.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawProjectConfig {
    schema_version: u32,
    project: Option<String>,
    services: BTreeMap<String, RawServiceConfig>,
}

impl RawProjectConfig {
    /// Returns the declared schema version.
    pub(crate) const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Returns the optional exact project name.
    pub(crate) fn project(&self) -> Option<&str> {
        self.project.as_deref()
    }

    /// Returns services keyed by their exact declared identities.
    pub(crate) fn services(&self) -> &BTreeMap<String, RawServiceConfig> {
        &self.services
    }

    pub(super) fn services_mut(&mut self) -> &mut BTreeMap<String, RawServiceConfig> {
        &mut self.services
    }
}
