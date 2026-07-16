use super::{ArtifactLock, ArtifactLockImage, RawProjectConfig, artifact_source};
use crate::control_plane::{PRESET_ARTIFACT_CATALOG_REVISION, resolve_preset_artifact};
use std::collections::BTreeMap;

/// Resolves every lockable service artifact into one complete immutable lock.
pub(crate) fn generate_artifact_lock<Resolve>(
    config: &RawProjectConfig,
    resolve: Resolve,
) -> Result<ArtifactLock, String>
where
    Resolve: FnOnce(&BTreeMap<String, String>) -> Result<BTreeMap<String, String>, String>,
{
    let mut images = BTreeMap::new();
    let mut lock_sources = BTreeMap::new();
    let mut mutable_references = BTreeMap::new();
    let mut uses_catalog = false;

    for (service_id, service) in config.services() {
        let source = artifact_source(service)
            .ok_or_else(|| format!("service '{service_id}' has no lockable artifact source"))?;
        if let Some(image) = service.image() {
            if is_immutable_registry_reference(image) {
                images.insert(
                    service_id.clone(),
                    ArtifactLockImage::new(source, image.to_owned()),
                );
            } else {
                lock_sources.insert(service_id.clone(), source);
                mutable_references.insert(service_id.clone(), image.to_owned());
            }
            continue;
        }

        let preset = service
            .preset()
            .ok_or_else(|| format!("service '{service_id}' has neither an image nor a preset"))?;
        let Some(artifact) = resolve_preset_artifact(preset, service.version())
            .map_err(|error| error.to_string())?
        else {
            continue;
        };
        uses_catalog = true;
        lock_sources.insert(service_id.clone(), source);
        mutable_references.insert(service_id.clone(), artifact.reference().to_owned());
    }

    if !mutable_references.is_empty() {
        let resolved = resolve(&mutable_references)?;
        if resolved.keys().ne(mutable_references.keys()) {
            return Err("image resolver returned a different key set than requested".to_owned());
        }
        for (service_id, resolved) in resolved {
            if !is_immutable_registry_reference(&resolved) {
                return Err(format!(
                    "image resolver returned a mutable resolution for service '{service_id}'"
                ));
            }
            let source = lock_sources
                .get(&service_id)
                .cloned()
                .ok_or_else(|| format!("image resolver returned unrequested key '{service_id}'"))?;
            images.insert(service_id, ArtifactLockImage::new(source, resolved));
        }
    }

    let lock = ArtifactLock::new(images);
    Ok(if uses_catalog {
        lock.with_catalog_revision(PRESET_ARTIFACT_CATALOG_REVISION)
    } else {
        lock
    })
}

fn is_immutable_registry_reference(image: &str) -> bool {
    image
        .rsplit_once("@sha256:")
        .is_some_and(|(repository, digest)| {
            !repository.is_empty()
                && digest.len() == 64
                && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}
