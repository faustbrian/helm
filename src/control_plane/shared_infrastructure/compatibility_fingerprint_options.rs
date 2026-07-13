use super::{IsolationCapability, PersistenceMode};
use serde::Serialize;
use std::collections::BTreeMap;

/// Immutable fields deciding whether projects may share one service instance.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct CompatibilityFingerprintOptions {
    pub(crate) implementation: String,
    pub(crate) major_version: String,
    pub(crate) image_digest: String,
    pub(crate) extensions: Vec<String>,
    pub(crate) immutable_settings: BTreeMap<String, String>,
    pub(crate) persistence: PersistenceMode,
    pub(crate) isolation: IsolationCapability,
    pub(crate) platform_architecture: Option<String>,
}
