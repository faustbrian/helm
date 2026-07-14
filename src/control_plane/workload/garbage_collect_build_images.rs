use super::{BuildImageGarbageCollectionOptions, WorkloadReconcileError};
use crate::control_plane::engine::{
    EngineError, ImageDiscovery, ImageId, ImageManager, ObservedResourceOwnership, OwnedImage,
    ResourceKind, RetentionClass, reconstruct_owned_image,
};

/// Removes expired, inactive, unreferenced derived images after full ownership validation.
pub(crate) async fn garbage_collect_build_images<E>(
    engine: &mut E,
    options: BuildImageGarbageCollectionOptions<'_>,
) -> Result<Vec<ImageId>, WorkloadReconcileError>
where
    E: ImageDiscovery + ImageManager,
{
    if options.installation_id.is_empty()
        || options.schema_version == 0
        || options.now_unix_seconds < 0
        || options.retention_seconds < 0
        || options.active_image_ids.iter().any(String::is_empty)
    {
        return Err(WorkloadReconcileError::InvalidRequest {
            detail: "build-image garbage-collection policy is invalid".to_owned(),
        });
    }
    let observed = engine
        .discover_managed_images()
        .await
        .map_err(|error| engine_error("discover build images", error))?;
    let mut owned = observed
        .iter()
        .filter_map(|observed| {
            match reconstruct_owned_image(observed, options.installation_id, options.schema_version)
            {
                Ok(image) => Some(Ok((observed, image))),
                Err(
                    ObservedResourceOwnership::Unmanaged
                    | ObservedResourceOwnership::ForeignInstallation { .. },
                ) => None,
                Err(ownership) => Some(Err(WorkloadReconcileError::Conflict {
                    detail: format!(
                        "build image '{}' has invalid ownership: {ownership:?}",
                        observed.id().as_str()
                    ),
                })),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let Some((_, image)) = owned.iter().find(|(_, image)| !is_build_cache(image)) {
        return Err(WorkloadReconcileError::Conflict {
            detail: format!(
                "managed image '{}' is not a disposable build cache",
                image.id().as_str()
            ),
        });
    }
    if let Some((observed, _)) = owned
        .iter()
        .find(|(observed, _)| observed.created_at_unix_seconds() < 0)
    {
        return Err(WorkloadReconcileError::Conflict {
            detail: format!(
                "build image '{}' has an invalid creation time",
                observed.id().as_str()
            ),
        });
    }
    owned.sort_by(|(_, left), (_, right)| left.id().as_str().cmp(right.id().as_str()));
    let expiration = options
        .now_unix_seconds
        .saturating_sub(options.retention_seconds);
    let mut removed = Vec::new();

    for (observed, image) in owned {
        if observed.created_at_unix_seconds() > expiration
            || observed.container_count() != 0
            || options
                .active_image_ids
                .iter()
                .any(|active| active == image.id().as_str())
        {
            continue;
        }
        engine
            .remove_image(&image)
            .await
            .map_err(|error| engine_error("remove expired build image", error))?;
        removed.push(image.id().clone());
    }

    Ok(removed)
}

fn is_build_cache(image: &OwnedImage) -> bool {
    image.metadata().kind() == ResourceKind::Build
        && image.metadata().retention() == RetentionClass::BuildCache
}

fn engine_error(action: &str, error: EngineError) -> WorkloadReconcileError {
    WorkloadReconcileError::Engine {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}
