use crate::control_plane::engine::{
    ContainerCreateOptions, ContainerNetworkIsolation, NetworkDiscovery, OwnedContainer,
    reconstruct_owned_network,
};
use crate::control_plane::network::matches_global_network;
use crate::control_plane::shared_infrastructure::SharedInfrastructureReconcileError;

/// Repairs the exact private-network attachment after interrupted maintenance.
pub(crate) async fn reconnect_rabbitmq_network<E>(
    engine: &E,
    container: &OwnedContainer,
    request: &ContainerCreateOptions,
    installation_id: &str,
    schema_version: u32,
) -> Result<(), SharedInfrastructureReconcileError>
where
    E: ContainerNetworkIsolation + NetworkDiscovery,
{
    if request.network().is_none() {
        return Err(SharedInfrastructureReconcileError::InvalidRequest {
            detail: "RabbitMQ shared service requires one private Engine network".to_owned(),
        });
    }
    let networks = engine.discover_managed_networks().await.map_err(|error| {
        SharedInfrastructureReconcileError::Engine {
            action: "RabbitMQ network discovery".to_owned(),
            detail: error.to_string(),
        }
    })?;
    let matching = networks
        .iter()
        .filter_map(|observed| {
            reconstruct_owned_network(observed, installation_id, schema_version).ok()
        })
        .filter(|network| matches_global_network(network, installation_id))
        .collect::<Vec<_>>();
    let [network] = matching.as_slice() else {
        return Err(SharedInfrastructureReconcileError::Conflict {
            detail: format!(
                "RabbitMQ shared service requires exactly one owned private network; found {}",
                matching.len()
            ),
        });
    };
    engine
        .reconnect_container_network(container, network, request.name())
        .await
        .map_err(|error| SharedInfrastructureReconcileError::Engine {
            action: "RabbitMQ network attachment repair".to_owned(),
            detail: error.to_string(),
        })
}
