use crate::control_plane::retention::BackupResourceIdentity;
use std::path::Path;
use std::time::Duration;

/// Durable recovery context for one exact accepted-v7 volume archive.
pub(crate) struct V7VolumeBackupOptions<'operation> {
    pub(crate) identity: &'operation BackupResourceIdentity,
    pub(crate) volume_name: &'operation str,
    pub(crate) backup_root: &'operation Path,
    pub(crate) created_at_unix_seconds: i64,
    pub(crate) verified_at_unix_seconds: i64,
    pub(crate) timeout: Duration,
}
