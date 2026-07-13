use serde::Deserialize;
use std::collections::BTreeMap;

/// Strict serialized shape of one v8 `.stackctl.lock.yaml` file.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArtifactLock {
    schema_version: u32,
    images: BTreeMap<String, ArtifactLockImage>,
}

impl ArtifactLock {
    pub(crate) const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub(crate) const fn images(&self) -> &BTreeMap<String, ArtifactLockImage> {
        &self.images
    }
}

/// One source-matched immutable image resolution.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArtifactLockImage {
    source: String,
    resolved: String,
}

impl ArtifactLockImage {
    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn resolved(&self) -> &str {
        &self.resolved
    }
}
