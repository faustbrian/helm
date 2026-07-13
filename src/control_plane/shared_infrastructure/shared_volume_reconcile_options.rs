use crate::control_plane::engine::VolumeCreateOptions;

/// Ownership scope and desired request for one shared data volume.
pub(crate) struct SharedVolumeReconcileOptions<'operation> {
    pub(crate) request: &'operation VolumeCreateOptions,
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
}
