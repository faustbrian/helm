use crate::control_plane::state::{CredentialRecord, LogicalResourceRecord};
use std::path::Path;
use std::time::Duration;

/// Complete immutable input for one scoped RabbitMQ topology backup.
pub(crate) struct RabbitMqBackupOptions<'operation> {
    pub(crate) logical_resource: &'operation LogicalResourceRecord,
    pub(crate) credential: &'operation CredentialRecord,
    pub(crate) installation_id: &'operation str,
    pub(crate) created_at_unix_seconds: i64,
    pub(crate) backup_root: &'operation Path,
    pub(crate) timeout: Duration,
}
