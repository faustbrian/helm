use super::{
    NetworkReconcileError, NetworkReconcileOptions, NetworkReconcileResult,
    NetworksReconcileOptions, reconcile_network::reconcile_network_from_observed,
};
use crate::control_plane::engine::{EngineError, NetworkDiscovery, NetworkManager};
use std::collections::BTreeSet;

/// Reconciles a complete network set from one bounded Engine discovery.
pub(crate) async fn reconcile_networks<Engine>(
    engine: &mut Engine,
    options: NetworksReconcileOptions<'_>,
) -> Result<Vec<NetworkReconcileResult>, NetworkReconcileError>
where
    Engine: NetworkDiscovery + NetworkManager,
{
    let mut scopes = BTreeSet::new();
    for request in options.requests {
        let scope = (
            request.metadata().project_id().map(str::to_owned),
            request.metadata().resource_id().map(str::to_owned),
        );
        if !scopes.insert(scope) {
            return Err(NetworkReconcileError::InvalidRequest {
                detail: "desired managed network scopes must be unique".to_owned(),
            });
        }
    }
    let observed = engine
        .discover_managed_networks()
        .await
        .map_err(discovery_error)?;
    let mut results = Vec::with_capacity(options.requests.len());
    for request in options.requests {
        results.push(
            reconcile_network_from_observed(
                engine,
                &observed,
                NetworkReconcileOptions {
                    request,
                    installation_id: options.installation_id,
                    schema_version: options.schema_version,
                },
            )
            .await?,
        );
    }

    Ok(results)
}

fn discovery_error(error: EngineError) -> NetworkReconcileError {
    NetworkReconcileError::EngineUnavailable {
        action: "discovery".to_owned(),
        detail: error.to_string(),
    }
}
