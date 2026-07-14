use super::QueuedProjectBackup;
use crate::control_plane::engine::EngineError;
use crate::control_plane::state::{CredentialRecord, LogicalResourceRecord};
use std::path::PathBuf;
use std::time::Duration;

/// Complete runtime-only state for one queued project recovery point.
pub(crate) struct ProjectBackupExecutionOptions {
    pub(crate) operation: QueuedProjectBackup,
    pub(crate) logical_resource: Result<LogicalResourceRecord, EngineError>,
    pub(crate) credential: Result<CredentialRecord, EngineError>,
    pub(crate) administrator: Result<Option<CredentialRecord>, EngineError>,
    pub(crate) installation_id: String,
    pub(crate) schema_version: u32,
    pub(crate) backup_root: PathBuf,
    pub(crate) created_at_unix_seconds: i64,
    pub(crate) timeout: Duration,
}
