use super::{
    GlobalNetworkReconcileAction, GlobalNetworkReconcileError, GlobalNetworkReconcileOptions,
    GlobalNetworkReconcileResult,
};
use crate::control_plane::engine::{
    EngineError, NetworkDiscovery, NetworkManager, ObservedResourceOwnership, ResourceKind,
    RetentionClass, reconstruct_owned_network,
};

/// Creates or adopts exactly one current-installation private network.
pub(crate) async fn reconcile_global_network<Engine>(
    engine: &mut Engine,
    options: GlobalNetworkReconcileOptions<'_>,
) -> Result<GlobalNetworkReconcileResult, GlobalNetworkReconcileError>
where
    Engine: NetworkDiscovery + NetworkManager,
{
    validate_request(&options)?;
    let observed = engine
        .discover_managed_networks()
        .await
        .map_err(|error| engine_unavailable("discovery", error))?;
    let mut owned = Vec::new();

    for network in &observed {
        match reconstruct_owned_network(network, options.installation_id, options.schema_version) {
            Ok(network) => owned.push(network),
            Err(ObservedResourceOwnership::ForeignInstallation { .. })
            | Err(ObservedResourceOwnership::Unmanaged) => {}
            Err(ownership) => {
                return Err(GlobalNetworkReconcileError::Conflict {
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
                return Err(GlobalNetworkReconcileError::Conflict {
                    detail: format!(
                        "network '{}' ownership does not match the desired installation network",
                        network.id().as_str()
                    ),
                });
            }

            Ok(GlobalNetworkReconcileResult::new(
                network.clone(),
                GlobalNetworkReconcileAction::Unchanged,
            ))
        }
        networks => Err(GlobalNetworkReconcileError::Conflict {
            detail: format!(
                "the installation network was observed {} times; refusing to guess",
                networks.len()
            ),
        }),
    }
}

async fn create_network<Engine>(
    engine: &mut Engine,
    options: &GlobalNetworkReconcileOptions<'_>,
) -> Result<GlobalNetworkReconcileResult, GlobalNetworkReconcileError>
where
    Engine: NetworkManager,
{
    let network = engine
        .create_network(options.request)
        .await
        .map_err(|error| mutation_error("creation", error))?;
    if network.metadata() != options.request.metadata() {
        return Err(GlobalNetworkReconcileError::Mutation {
            action: "creation".to_owned(),
            detail: "Engine returned a network with unexpected ownership".to_owned(),
        });
    }

    Ok(GlobalNetworkReconcileResult::new(
        network,
        GlobalNetworkReconcileAction::Created,
    ))
}

fn validate_request(
    options: &GlobalNetworkReconcileOptions<'_>,
) -> Result<(), GlobalNetworkReconcileError> {
    let metadata = options.request.metadata();
    if metadata.kind() != ResourceKind::Network
        || metadata.project_id().is_some()
        || metadata.installation_id() != options.installation_id
        || metadata.schema_version() != options.schema_version
        || metadata.resource_id() != Some("private")
        || metadata.retention() != RetentionClass::Persistent
    {
        return Err(GlobalNetworkReconcileError::InvalidRequest {
            detail: "ownership must identify the active installation's persistent private network"
                .to_owned(),
        });
    }

    Ok(())
}

fn engine_unavailable(action: &str, error: EngineError) -> GlobalNetworkReconcileError {
    GlobalNetworkReconcileError::EngineUnavailable {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}

fn mutation_error(action: &str, error: EngineError) -> GlobalNetworkReconcileError {
    GlobalNetworkReconcileError::Mutation {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}
