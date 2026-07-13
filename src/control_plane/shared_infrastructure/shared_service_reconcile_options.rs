use crate::control_plane::engine::{ContainerCreateOptions, VolumeCreateOptions};

/// Complete Engine resources and ownership scope for one shared service.
pub(crate) struct SharedServiceReconcileOptions<'operation> {
    pub(crate) request: &'operation ContainerCreateOptions,
    pub(crate) volume: Option<&'operation VolumeCreateOptions>,
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
}
