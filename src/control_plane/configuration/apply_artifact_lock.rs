use super::{ArtifactLock, ArtifactLockError, RawProjectConfig, artifact_source};
use crate::control_plane::engine::{ImmutableImageReference, is_immutable_image_identity};
use crate::control_plane::service_strategy::{
    PRESET_ARTIFACT_CATALOG_REVISION, resolve_preset_artifact,
};
use std::path::Path;

/// Applies exact source-matched immutable resolutions before desired-state planning.
pub(crate) fn apply_artifact_lock(
    config: &mut RawProjectConfig,
    lock: &ArtifactLock,
    lock_path: &Path,
) -> Result<(), ArtifactLockError> {
    for (service_id, resolution) in lock.images() {
        let service = config.services_mut().get_mut(service_id).ok_or_else(|| {
            ArtifactLockError::new(
                lock_path.to_path_buf(),
                format!("image entry '{service_id}' does not match a declared service"),
            )
        })?;
        let expected_source = artifact_source(service).ok_or_else(|| {
            ArtifactLockError::new(
                lock_path.to_path_buf(),
                format!("service '{service_id}' does not declare an image or preset"),
            )
        })?;
        let preset_artifact = if service.image().is_none() {
            service
                .preset()
                .map(|preset| resolve_preset_artifact(preset, service.version()))
                .transpose()
                .map_err(|error| {
                    ArtifactLockError::new(lock_path.to_path_buf(), error.to_string())
                })?
                .flatten()
        } else {
            None
        };

        if resolution.source() != expected_source {
            return Err(ArtifactLockError::new(
                lock_path.to_path_buf(),
                format!(
                    "image entry '{service_id}' source does not match: expected \
                     '{expected_source}', found '{}'",
                    resolution.source()
                ),
            ));
        }
        if ImmutableImageReference::new(resolution.resolved()).is_err() {
            return Err(ArtifactLockError::new(
                lock_path.to_path_buf(),
                format!(
                    "image entry '{service_id}' resolved value '{}' must use an immutable sha256 \
                     digest",
                    resolution.resolved()
                ),
            ));
        }
        if let Some(artifact) = &preset_artifact {
            if lock.catalog_revision() != Some(PRESET_ARTIFACT_CATALOG_REVISION) {
                return Err(ArtifactLockError::new(
                    lock_path.to_path_buf(),
                    format!(
                        "preset image entry '{service_id}' requires catalog_revision \
                         '{PRESET_ARTIFACT_CATALOG_REVISION}'"
                    ),
                ));
            }
            let expected_repository = mutable_repository(artifact.reference());
            let resolved_repository = resolution
                .resolved()
                .rsplit_once("@sha256:")
                .map(|(repository, _)| repository)
                .unwrap_or_default();
            if resolved_repository != expected_repository {
                return Err(ArtifactLockError::new(
                    lock_path.to_path_buf(),
                    format!(
                        "preset image entry '{service_id}' resolved repository must be \
                         '{expected_repository}', found '{resolved_repository}'"
                    ),
                ));
            }
        }
        if is_immutable_image_identity(&expected_source) && resolution.resolved() != expected_source
        {
            return Err(ArtifactLockError::new(
                lock_path.to_path_buf(),
                format!(
                    "image entry '{service_id}' cannot replace immutable source \
                     '{expected_source}' with '{}'",
                    resolution.resolved()
                ),
            ));
        }

        if service.version().is_none()
            && let Some(artifact) = &preset_artifact
        {
            service.set_version(artifact.version().to_owned());
        }
        service.set_image(resolution.resolved().to_owned());
    }

    Ok(())
}

fn mutable_repository(reference: &str) -> &str {
    let last_slash = reference.rfind('/');
    match reference.rfind(':') {
        Some(colon) if last_slash.is_none_or(|slash| colon > slash) => &reference[..colon],
        _ => reference,
    }
}
