use super::{ObservedResourceOwnership, ObservedVolume, OwnedVolume, classify_observed_resource};

/// Rebuilds a mutable volume handle only from complete current-installation labels.
pub(crate) fn reconstruct_owned_volume(
    observed: &ObservedVolume,
    current_installation_id: &str,
    supported_schema_version: u32,
) -> Result<OwnedVolume, ObservedResourceOwnership> {
    match classify_observed_resource(
        observed.labels(),
        current_installation_id,
        supported_schema_version,
    ) {
        ObservedResourceOwnership::Owned(metadata) => {
            Ok(OwnedVolume::new(observed.name(), metadata))
        }
        ownership => Err(ownership),
    }
}
