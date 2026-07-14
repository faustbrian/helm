use crate::control_plane::shared_infrastructure::RedisFlavor;
use crate::control_plane::state::{CredentialRecord, LogicalResourceRecord, RecoveryPointRecord};
use std::time::Duration;

/// Complete bounded input for one verified Redis-compatible prefix restore.
pub(crate) struct RedisRestoreOptions<'operation> {
    pub(crate) flavor: RedisFlavor,
    pub(crate) logical_resource: &'operation LogicalResourceRecord,
    pub(crate) credential: &'operation CredentialRecord,
    pub(crate) administrator: &'operation CredentialRecord,
    pub(crate) recovery_point: &'operation RecoveryPointRecord,
    pub(crate) prefix: &'operation str,
    pub(crate) installation_id: &'operation str,
    pub(crate) restored_at_unix_seconds: i64,
    pub(crate) timeout: Duration,
}
