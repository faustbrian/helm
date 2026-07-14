use crate::control_plane::engine::{ContainerCreateOptions, VolumeCreateOptions};
use std::path::Path;
use std::time::Duration;

/// Exact prepared-v8 target and verified accepted-v7 archive to restore.
pub(crate) struct V7VolumeTargetRestoreOptions<'operation> {
    pub(crate) desired_container: &'operation ContainerCreateOptions,
    pub(crate) desired_volume: &'operation VolumeCreateOptions,
    pub(crate) expected_mount_target: &'operation str,
    pub(crate) archive: &'operation Path,
    pub(crate) timeout: Duration,
}
