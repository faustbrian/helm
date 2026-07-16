use super::{
    NetworkReconcileAction, NetworkReconcileError, NetworkReconcileOptions, NetworkReconcileResult,
};
use crate::control_plane::engine::{
    EngineError, ManagedResourceMetadata, NetworkManager, ObservedNetwork,
    ObservedResourceOwnership, OwnedNetwork, ResourceKind, RetentionClass,
    reconstruct_owned_network,
};

/// Creates or adopts one exact installation- or project-scoped network.
#[cfg(test)]
pub(crate) async fn reconcile_network<Engine>(
    engine: &mut Engine,
    options: NetworkReconcileOptions<'_>,
) -> Result<NetworkReconcileResult, NetworkReconcileError>
where
    Engine: crate::control_plane::engine::NetworkDiscovery + NetworkManager,
{
    validate_request(&options)?;
    let observed = engine
        .discover_managed_networks()
        .await
        .map_err(|error| engine_unavailable("discovery", error))?;
    reconcile_network_from_observed(engine, &observed, options).await
}

pub(super) async fn reconcile_network_from_observed<Engine>(
    engine: &mut Engine,
    observed: &[ObservedNetwork],
    options: NetworkReconcileOptions<'_>,
) -> Result<NetworkReconcileResult, NetworkReconcileError>
where
    Engine: NetworkManager,
{
    validate_request(&options)?;
    let mut owned = Vec::new();

    for network in observed {
        match reconstruct_owned_network(network, options.installation_id, options.schema_version) {
            Ok(network) if has_desired_scope(&network, options.request.metadata()) => {
                owned.push(network);
            }
            Ok(_) => {}
            Err(ObservedResourceOwnership::ForeignInstallation { .. })
            | Err(ObservedResourceOwnership::Unmanaged) => {}
            Err(ownership) => {
                return Err(NetworkReconcileError::Conflict {
                    detail: format!(
                        "a managed network has ownership that cannot be proven: {ownership:?}"
                    ),
                });
            }
        }
    }

    match owned.as_slice() {
        [] => create_network(engine, &options).await,
        [network] => {
            if network.metadata() != options.request.metadata() {
                return Err(NetworkReconcileError::Conflict {
                    detail: format!(
                        "network '{}' ownership does not match the desired private network",
                        network.id().as_str()
                    ),
                });
            }

            Ok(NetworkReconcileResult::new(
                network.clone(),
                NetworkReconcileAction::Unchanged,
            ))
        }
        networks => Err(NetworkReconcileError::Conflict {
            detail: format!(
                "the desired private network was observed {} times; refusing to guess",
                networks.len()
            ),
        }),
    }
}

fn has_desired_scope(network: &OwnedNetwork, desired: &ManagedResourceMetadata) -> bool {
    network.metadata().kind() == desired.kind()
        && network.metadata().project_id() == desired.project_id()
        && network.metadata().resource_id() == desired.resource_id()
}

async fn create_network<Engine>(
    engine: &mut Engine,
    options: &NetworkReconcileOptions<'_>,
) -> Result<NetworkReconcileResult, NetworkReconcileError>
where
    Engine: NetworkManager,
{
    let network = engine
        .create_network(options.request)
        .await
        .map_err(|error| mutation_error("creation", error))?;
    if network.metadata() != options.request.metadata() {
        return Err(NetworkReconcileError::Mutation {
            action: "creation".to_owned(),
            detail: "Engine returned a network with unexpected ownership".to_owned(),
        });
    }

    Ok(NetworkReconcileResult::new(
        network,
        NetworkReconcileAction::Created,
    ))
}

fn validate_request(options: &NetworkReconcileOptions<'_>) -> Result<(), NetworkReconcileError> {
    let metadata = options.request.metadata();
    if metadata.kind() != ResourceKind::Network
        || metadata.installation_id() != options.installation_id
        || metadata.schema_version() != options.schema_version
        || metadata.resource_id() != Some("private")
        || metadata.retention() != RetentionClass::Persistent
    {
        return Err(NetworkReconcileError::InvalidRequest {
            detail: "ownership must identify an active persistent private network".to_owned(),
        });
    }

    if let Some(project_id) = metadata.project_id()
        && !options.request.name().ends_with(&format!("-{project_id}"))
    {
        return Err(NetworkReconcileError::InvalidRequest {
            detail: "project network name must end with its exact project identity".to_owned(),
        });
    }

    Ok(())
}

#[cfg(test)]
fn engine_unavailable(action: &str, error: EngineError) -> NetworkReconcileError {
    NetworkReconcileError::EngineUnavailable {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}

fn mutation_error(action: &str, error: EngineError) -> NetworkReconcileError {
    NetworkReconcileError::Mutation {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}
