use super::{ObservedImage, ObservedResourceOwnership, OwnedImage, classify_observed_resource};

/// Rebuilds a mutable image handle only from complete current-installation labels.
pub(crate) fn reconstruct_owned_image(
    observed: &ObservedImage,
    current_installation_id: &str,
    supported_schema_version: u32,
) -> Result<OwnedImage, ObservedResourceOwnership> {
    match classify_observed_resource(
        observed.labels(),
        current_installation_id,
        supported_schema_version,
    ) {
        ObservedResourceOwnership::Owned(metadata) => {
            Ok(OwnedImage::new(observed.id().clone(), *metadata))
        }
        ownership => Err(ownership),
    }
}
