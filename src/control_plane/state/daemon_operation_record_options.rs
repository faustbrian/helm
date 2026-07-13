use super::DaemonOperationStatus;

/// Complete durable fields for one new daemon operation.
pub(crate) struct DaemonOperationRecordOptions {
    pub(crate) operation_id: String,
    pub(crate) kind: String,
    pub(crate) payload_json: String,
    pub(crate) status: DaemonOperationStatus,
    pub(crate) created_at_unix_seconds: i64,
    pub(crate) updated_at_unix_seconds: i64,
}
