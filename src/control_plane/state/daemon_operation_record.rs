use super::{DaemonOperationRecordOptions, DaemonOperationStatus};

/// Backend-neutral durable payload and lifecycle for one daemon operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DaemonOperationRecord {
    operation_id: String,
    kind: String,
    payload_json: String,
    status: DaemonOperationStatus,
    created_at_unix_seconds: i64,
    updated_at_unix_seconds: i64,
}

impl DaemonOperationRecord {
    pub(crate) fn new(options: DaemonOperationRecordOptions) -> Self {
        Self {
            operation_id: options.operation_id,
            kind: options.kind,
            payload_json: options.payload_json,
            status: options.status,
            created_at_unix_seconds: options.created_at_unix_seconds,
            updated_at_unix_seconds: options.updated_at_unix_seconds,
        }
    }

    pub(crate) fn operation_id(&self) -> &str {
        &self.operation_id
    }

    pub(crate) fn kind(&self) -> &str {
        &self.kind
    }

    pub(crate) fn payload_json(&self) -> &str {
        &self.payload_json
    }

    pub(crate) const fn status(&self) -> DaemonOperationStatus {
        self.status
    }

    pub(crate) const fn created_at_unix_seconds(&self) -> i64 {
        self.created_at_unix_seconds
    }

    pub(crate) const fn updated_at_unix_seconds(&self) -> i64 {
        self.updated_at_unix_seconds
    }
}
