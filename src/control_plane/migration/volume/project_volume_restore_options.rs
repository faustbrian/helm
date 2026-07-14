use crate::control_plane::engine::{ContainerCreateOptions, VolumeCreateOptions};
use crate::control_plane::state::{RecoveryPointRecord, ResourceRecord};
use std::time::Duration;

/// Exact durable and desired identity for one dedicated-volume restore.
pub(crate) struct ProjectVolumeRestoreOptions<'operation> {
    pub(crate) resource: &'operation ResourceRecord,
    pub(crate) recovery_point: &'operation RecoveryPointRecord,
    pub(crate) desired_container: &'operation ContainerCreateOptions,
    pub(crate) desired_volume: &'operation VolumeCreateOptions,
    pub(crate) installation_id: &'operation str,
    pub(crate) project_id: &'operation str,
    pub(crate) service_id: &'operation str,
    pub(crate) verified_at_unix_seconds: i64,
    pub(crate) timeout: Duration,
}
