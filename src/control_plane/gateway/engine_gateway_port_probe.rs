use super::{GatewayError, GatewayPortAvailability, GatewayPortProbe};
use crate::control_plane::engine::PublishedPortBinding;
use std::net::{IpAddr, SocketAddr};

/// Port observer that attributes Engine bindings before probing the host.
pub(crate) struct EngineGatewayPortProbe<'inventory> {
    host_probe: &'inventory dyn GatewayPortProbe,
    bindings: &'inventory [PublishedPortBinding],
}

impl<'inventory> EngineGatewayPortProbe<'inventory> {
    pub(crate) const fn new(
        host_probe: &'inventory dyn GatewayPortProbe,
        bindings: &'inventory [PublishedPortBinding],
    ) -> Self {
        Self {
            host_probe,
            bindings,
        }
    }
}

impl GatewayPortProbe for EngineGatewayPortProbe<'_> {
    fn probe(&self, address: SocketAddr) -> Result<GatewayPortAvailability, GatewayError> {
        let mut owners = self
            .bindings
            .iter()
            .filter(|binding| binding.host_port() == address.port())
            .filter(|binding| binding_claims_address(binding.host_ip(), address.ip()))
            .map(|binding| {
                format!(
                    "Engine container '{}' (id '{}')",
                    binding.container_name(),
                    binding.container_id().as_str()
                )
            })
            .collect::<Vec<_>>();
        owners.sort();
        owners.dedup();

        if owners.is_empty() {
            return self.host_probe.probe(address);
        }

        Ok(GatewayPortAvailability::Occupied {
            owner: Some(owners.join(", ")),
        })
    }
}

fn binding_claims_address(binding: IpAddr, required: IpAddr) -> bool {
    binding == required
        || matches!(
            (binding, required),
            (IpAddr::V4(binding), IpAddr::V4(_)) if binding.is_unspecified()
        )
        || matches!(
            (binding, required),
            (IpAddr::V6(binding), IpAddr::V6(_)) if binding.is_unspecified()
        )
}
