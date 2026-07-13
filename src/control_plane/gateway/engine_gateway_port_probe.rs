use super::{GatewayError, GatewayPortAvailability, GatewayPortProbe};
use crate::control_plane::engine::{ContainerId, PublishedPortBinding};
use std::net::{IpAddr, SocketAddr};

/// Port observer that attributes Engine bindings before probing the host.
pub(crate) struct EngineGatewayPortProbe<'inventory> {
    host_probe: &'inventory dyn GatewayPortProbe,
    bindings: &'inventory [PublishedPortBinding],
    ignored_container_id: Option<&'inventory ContainerId>,
}

impl<'inventory> EngineGatewayPortProbe<'inventory> {
    pub(crate) const fn new(
        host_probe: &'inventory dyn GatewayPortProbe,
        bindings: &'inventory [PublishedPortBinding],
    ) -> Self {
        Self {
            host_probe,
            bindings,
            ignored_container_id: None,
        }
    }

    pub(crate) const fn ignoring(
        host_probe: &'inventory dyn GatewayPortProbe,
        bindings: &'inventory [PublishedPortBinding],
        container_id: &'inventory ContainerId,
    ) -> Self {
        Self {
            host_probe,
            bindings,
            ignored_container_id: Some(container_id),
        }
    }
}

impl GatewayPortProbe for EngineGatewayPortProbe<'_> {
    fn probe(&self, address: SocketAddr) -> Result<GatewayPortAvailability, GatewayError> {
        let mut owners = Vec::new();
        let mut ignored_claim = false;
        for binding in self.bindings.iter().filter(|binding| {
            binding.host_port() == address.port()
                && binding_claims_address(binding.host_ip(), address.ip())
        }) {
            if self.ignored_container_id == Some(binding.container_id()) {
                ignored_claim = true;
            } else {
                owners.push(format!(
                    "Engine container '{}' (id '{}')",
                    binding.container_name(),
                    binding.container_id().as_str()
                ));
            }
        }
        owners.sort();
        owners.dedup();

        if owners.is_empty() {
            if ignored_claim {
                return Ok(GatewayPortAvailability::Available);
            }
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
