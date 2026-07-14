use crate::control_plane::shared_infrastructure::RedisFlavor;
use crate::control_plane::state::{CredentialRecord, LogicalResourceRecord};
use std::path::Path;
use std::time::Duration;

/// Complete bounded input for one Redis-compatible logical snapshot.
pub(crate) struct RedisBackupOptions<'operation> {
    pub(crate) flavor: RedisFlavor,
    pub(crate) logical_resource: &'operation LogicalResourceRecord,
    pub(crate) credential: &'operation CredentialRecord,
    pub(crate) administrator: &'operation CredentialRecord,
    pub(crate) prefix: &'operation str,
    pub(crate) installation_id: &'operation str,
    pub(crate) created_at_unix_seconds: i64,
    pub(crate) backup_root: &'operation Path,
    pub(crate) timeout: Duration,
}
