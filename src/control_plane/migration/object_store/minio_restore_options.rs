use crate::control_plane::state::{CredentialRecord, LogicalResourceRecord, RecoveryPointRecord};
use std::time::Duration;

/// Complete bounded input for one verified MinIO current-object restore.
pub(crate) struct MinioRestoreOptions<'operation> {
    pub(crate) recovery_point: &'operation RecoveryPointRecord,
    pub(crate) logical_resource: &'operation LogicalResourceRecord,
    pub(crate) credential: &'operation CredentialRecord,
    pub(crate) installation_id: &'operation str,
    pub(crate) target_bucket_name: &'operation str,
    pub(crate) verified_at_unix_seconds: i64,
    pub(crate) timeout: Duration,
}
