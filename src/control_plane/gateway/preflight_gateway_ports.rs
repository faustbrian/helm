use super::{
    EngineGatewayPortProbe, GatewayError, GatewayPortProbe, verify_gateway_ports_available,
};
use crate::control_plane::engine::PublishedPortDiscovery;

/// Discovers Engine owners and validates every required gateway socket.
pub(crate) async fn preflight_gateway_ports(
    engine: &dyn PublishedPortDiscovery,
    host_probe: &dyn GatewayPortProbe,
) -> Result<(), GatewayError> {
    let bindings = engine
        .discover_published_tcp_ports()
        .await
        .map_err(|error| GatewayError::Preflight {
            detail: format!("could not inspect Engine gateway port ownership: {error}"),
        })?;
    let probe = EngineGatewayPortProbe::new(host_probe, &bindings);

    verify_gateway_ports_available(&probe)
}
