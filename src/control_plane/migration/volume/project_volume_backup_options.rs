use crate::control_plane::state::ResourceRecord;
use std::path::Path;
use std::time::Duration;

/// Complete immutable inputs for one owned dedicated-volume backup.
pub(crate) struct ProjectVolumeBackupOptions<'operation> {
    pub(crate) resource: &'operation ResourceRecord,
    pub(crate) installation_id: &'operation str,
    pub(crate) project_id: &'operation str,
    pub(crate) service_id: &'operation str,
    pub(crate) created_at_unix_seconds: i64,
    pub(crate) backup_root: &'operation Path,
    pub(crate) timeout: Duration,
}
