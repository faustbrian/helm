/// Complete immutable evidence for one verified logical recovery point.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecoveryPointRecordOptions {
    pub(crate) recovery_point_id: String,
    pub(crate) project_id: String,
    pub(crate) service_id: String,
    pub(crate) logical_resource_id: String,
    pub(crate) resource_kind: String,
    pub(crate) compatibility_fingerprint: String,
    pub(crate) reference: String,
    pub(crate) artifact_sha256: String,
    pub(crate) artifact_size_bytes: u64,
    pub(crate) created_at_unix_seconds: i64,
    pub(crate) verified_at_unix_seconds: i64,
}
