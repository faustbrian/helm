use super::{ObservedNetwork, ObservedResourceOwnership, OwnedNetwork, classify_observed_resource};

/// Rebuilds a mutable network handle only from complete current-installation labels.
pub(crate) fn reconstruct_owned_network(
    observed: &ObservedNetwork,
    current_installation_id: &str,
    supported_schema_version: u32,
) -> Result<OwnedNetwork, ObservedResourceOwnership> {
    match classify_observed_resource(
        observed.labels(),
        current_installation_id,
        supported_schema_version,
    ) {
        ObservedResourceOwnership::Owned(metadata) => {
            Ok(OwnedNetwork::new(observed.id().clone(), *metadata))
        }
        ownership => Err(ownership),
    }
}
