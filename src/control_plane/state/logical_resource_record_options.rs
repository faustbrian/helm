use super::ResourceLifecycle;

/// Complete durable ownership and lifecycle fields for one logical tenant resource.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LogicalResourceRecordOptions {
    pub(crate) logical_resource_id: String,
    pub(crate) shared_resource_id: String,
    pub(crate) project_id: String,
    pub(crate) service_id: String,
    pub(crate) kind: String,
    pub(crate) compatibility_fingerprint: String,
    pub(crate) desired_revision: String,
    pub(crate) lifecycle: ResourceLifecycle,
    pub(crate) orphaned_at_unix_seconds: Option<i64>,
}
