use super::{ResourceLifecycle, ResourceRetention};

/// Complete durable ownership and retention fields for one managed resource.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResourceRecordOptions {
    pub(crate) resource_id: String,
    pub(crate) installation_id: String,
    pub(crate) kind: String,
    pub(crate) compatibility_fingerprint: String,
    pub(crate) project_id: Option<String>,
    pub(crate) schema_version: u32,
    pub(crate) desired_revision: String,
    pub(crate) retention: ResourceRetention,
    pub(crate) lifecycle: ResourceLifecycle,
    pub(crate) orphaned_at_unix_seconds: Option<i64>,
}
