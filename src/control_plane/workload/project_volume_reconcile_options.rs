use crate::control_plane::engine::VolumeCreateOptions;

/// Exact ownership inputs for one retained project-volume convergence.
pub(crate) struct ProjectVolumeReconcileOptions<'operation> {
    pub(crate) request: &'operation VolumeCreateOptions,
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
}
