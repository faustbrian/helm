use crate::control_plane::engine::{ObservedVolume, VolumeCreateOptions};
use std::num::NonZeroUsize;

/// Complete mutation boundary for a retained project-volume batch.
pub(crate) struct ProjectVolumesReconcileOptions<'request> {
    pub(crate) requests: &'request [VolumeCreateOptions],
    pub(crate) observed: &'request [ObservedVolume],
    pub(crate) installation_id: &'request str,
    pub(crate) schema_version: u32,
    pub(crate) concurrency: NonZeroUsize,
}
