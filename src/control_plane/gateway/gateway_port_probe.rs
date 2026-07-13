use super::{GatewayError, GatewayPortAvailability};
use std::net::SocketAddr;

/// Replaceable, non-mutating observer for one required gateway socket.
pub(crate) trait GatewayPortProbe {
    fn probe(&self, address: SocketAddr) -> Result<GatewayPortAvailability, GatewayError>;
}
