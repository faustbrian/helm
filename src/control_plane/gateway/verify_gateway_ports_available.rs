use super::{GatewayError, GatewayPortAvailability, GatewayPortProbe};
use std::net::{Ipv4Addr, SocketAddr};

const REQUIRED_PORTS: [u16; 2] = [80, 443];

/// Fails before Engine mutation when a required loopback listener is occupied.
pub(crate) fn verify_gateway_ports_available(
    probe: &dyn GatewayPortProbe,
) -> Result<(), GatewayError> {
    let mut conflicts = Vec::new();

    for port in REQUIRED_PORTS {
        for address in [SocketAddr::from((Ipv4Addr::LOCALHOST, port))] {
            if let GatewayPortAvailability::Occupied { owner } = probe.probe(address)? {
                let owner =
                    owner.unwrap_or_else(|| "an unknown host process or Engine binding".to_owned());
                conflicts.push(format!("- {address} is occupied by {owner}"));
            }
        }
    }

    if conflicts.is_empty() {
        return Ok(());
    }

    Err(GatewayError::Preflight {
        detail: format!(
            "gateway cannot bind required loopback ports:\n{}\n\
             stop or reconfigure each owner; Stackctl will not choose alternate ports",
            conflicts.join("\n")
        ),
    })
}
