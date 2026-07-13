use crate::control_plane::engine::NetworkCreateOptions;

/// Desired request and ownership scope for the one installation-wide network.
pub(crate) struct GlobalNetworkReconcileOptions<'operation> {
    pub(crate) request: &'operation NetworkCreateOptions,
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
}
