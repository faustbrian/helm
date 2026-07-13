use super::{GatewayError, GatewayPortAvailability, GatewayPortProbe};
use std::io::ErrorKind;
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

const PROBE_TIMEOUT: Duration = Duration::from_millis(250);

/// Host socket observer that does not bind or mutate privileged ports.
pub(crate) struct SystemGatewayPortProbe;

impl GatewayPortProbe for SystemGatewayPortProbe {
    fn probe(&self, address: SocketAddr) -> Result<GatewayPortAvailability, GatewayError> {
        match TcpStream::connect_timeout(&address, PROBE_TIMEOUT) {
            Ok(_) => Ok(GatewayPortAvailability::Occupied { owner: None }),
            Err(error) if error.kind() == ErrorKind::ConnectionRefused => {
                Ok(GatewayPortAvailability::Available)
            }
            Err(error) => Err(GatewayError::Preflight {
                detail: format!("could not inspect required gateway socket {address}: {error}"),
            }),
        }
    }
}
