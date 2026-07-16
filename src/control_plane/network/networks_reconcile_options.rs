use crate::control_plane::engine::NetworkCreateOptions;

/// Complete desired network set and ownership scope for one Engine pass.
pub(crate) struct NetworksReconcileOptions<'operation> {
    pub(crate) requests: &'operation [NetworkCreateOptions],
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
}
