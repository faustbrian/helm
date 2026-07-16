use crate::control_plane::engine::NetworkCreateOptions;

/// Desired request and ownership scope for one managed private network.
pub(crate) struct NetworkReconcileOptions<'operation> {
    pub(crate) request: &'operation NetworkCreateOptions,
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
}
