use super::GatewayPortProbe;
use crate::control_plane::engine::ContainerCreateOptions;

/// Complete boundary inputs for one singleton gateway reconciliation pass.
pub(crate) struct GatewayReconcileOptions<'operation> {
    pub(crate) request: &'operation ContainerCreateOptions,
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
    pub(crate) host_probe: &'operation dyn GatewayPortProbe,
}
