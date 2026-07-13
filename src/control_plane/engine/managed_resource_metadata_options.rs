use super::{ResourceKind, RetentionClass};

/// Complete immutable ownership fields required for an Engine resource.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ManagedResourceMetadataOptions {
    pub(crate) installation_id: String,
    pub(crate) kind: ResourceKind,
    pub(crate) project_id: Option<String>,
    pub(crate) compatibility_fingerprint: String,
    pub(crate) schema_version: u32,
    pub(crate) desired_revision: String,
    pub(crate) retention: RetentionClass,
}
