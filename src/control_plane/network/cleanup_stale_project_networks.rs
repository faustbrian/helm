use super::{
    StaleProjectNetworkCleanupError, StaleProjectNetworkCleanupOptions, stale_project_networks,
};
use crate::control_plane::engine::{
    ContainerNetworkIsolation, NetworkManager, ObservedResourceOwnership, OwnedContainer,
    reconstruct_owned_container,
};
use std::collections::BTreeMap;

/// Detaches exact owned containers and removes data-free networks for absent projects.
pub(crate) async fn cleanup_stale_project_networks<Engine>(
    engine: &mut Engine,
    options: StaleProjectNetworkCleanupOptions<'_>,
) -> Result<usize, StaleProjectNetworkCleanupError>
where
    Engine: ContainerNetworkIsolation + NetworkManager,
{
    let stale = stale_project_networks(
        options.observed_networks,
        options.installation_id,
        options.schema_version,
        options.active_project_ids,
    )
    .map_err(|detail| StaleProjectNetworkCleanupError::Ownership { detail })?;
    let mut containers = BTreeMap::<String, OwnedContainer>::new();
    for observed in options.observed_containers {
        match reconstruct_owned_container(observed, options.installation_id, options.schema_version)
        {
            Ok(container) => {
                containers.insert(container.id().as_str().to_owned(), container);
            }
            Err(ObservedResourceOwnership::ForeignInstallation { .. })
            | Err(ObservedResourceOwnership::Unmanaged) => {}
            Err(ownership) => {
                return Err(StaleProjectNetworkCleanupError::Ownership {
                    detail: format!("managed container ownership cannot be proven: {ownership:?}"),
                });
            }
        }
    }
    containers.insert(
        options.gateway.id().as_str().to_owned(),
        options.gateway.clone(),
    );

    for network in &stale {
        let Some(project_id) = network.metadata().project_id() else {
            return Err(StaleProjectNetworkCleanupError::Ownership {
                detail: "stale project network lost its project identity".to_owned(),
            });
        };
        for container in containers.values().filter(|container| {
            container
                .metadata()
                .project_id()
                .is_none_or(|container_project| container_project == project_id)
        }) {
            engine
                .disconnect_container_network(container, network)
                .await
                .map_err(|error| StaleProjectNetworkCleanupError::Engine {
                    action: format!("detach for project '{project_id}'"),
                    detail: error.to_string(),
                })?;
        }
        engine.remove_network(network).await.map_err(|error| {
            StaleProjectNetworkCleanupError::Engine {
                action: format!("removal for project '{project_id}'"),
                detail: error.to_string(),
            }
        })?;
    }

    Ok(stale.len())
}
