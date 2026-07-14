use super::global_network_request;
use crate::control_plane::engine::OwnedNetwork;

/// Matches only the canonical private network owned by one installation.
pub(crate) fn matches_global_network(network: &OwnedNetwork, installation_id: &str) -> bool {
    global_network_request(installation_id)
        .is_ok_and(|request| network.metadata() == request.metadata())
}
