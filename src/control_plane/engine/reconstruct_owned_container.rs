use super::{
    ObservedContainer, ObservedResourceOwnership, OwnedContainer, classify_observed_resource,
};

/// Rebuilds a mutable container handle only from complete current-installation labels.
pub(crate) fn reconstruct_owned_container(
    observed: &ObservedContainer,
    current_installation_id: &str,
    supported_schema_version: u32,
) -> Result<OwnedContainer, ObservedResourceOwnership> {
    match classify_observed_resource(
        observed.labels(),
        current_installation_id,
        supported_schema_version,
    ) {
        ObservedResourceOwnership::Owned(metadata) => {
            Ok(OwnedContainer::new(observed.id().clone(), metadata))
        }
        ownership => Err(ownership),
    }
}
