use super::{
    SharedInfrastructureReconcileError, SharedVolumeReconcileAction, SharedVolumeReconcileOptions,
    SharedVolumeReconcileResult,
};
use crate::control_plane::engine::{
    EngineError, ObservedResourceOwnership, ResourceKind, RetentionClass, VolumeDiscovery,
    VolumeManager, reconstruct_owned_volume,
};

/// Creates or adopts one exact shared data volume without ordinary deletion.
pub(crate) async fn reconcile_shared_volume<E>(
    engine: &mut E,
    options: SharedVolumeReconcileOptions<'_>,
) -> Result<SharedVolumeReconcileResult, SharedInfrastructureReconcileError>
where
    E: VolumeDiscovery + VolumeManager,
{
    validate_request(&options)?;
    let observed = engine
        .discover_managed_volumes()
        .await
        .map_err(|error| engine_error("volume discovery", error))?;
    let matching = observed
        .iter()
        .filter(|volume| volume.name() == options.request.name())
        .collect::<Vec<_>>();

    match matching.as_slice() {
        [] => {
            let volume = engine
                .create_volume(options.request)
                .await
                .map_err(|error| engine_error("volume creation", error))?;
            if volume.name() != options.request.name()
                || volume.metadata() != options.request.metadata()
            {
                return Err(SharedInfrastructureReconcileError::Engine {
                    action: "volume creation".to_owned(),
                    detail: "Engine returned a shared volume with unexpected ownership".to_owned(),
                });
            }

            Ok(SharedVolumeReconcileResult::new(
                volume,
                SharedVolumeReconcileAction::Created,
            ))
        }
        [observed] => {
            let volume =
                reconstruct_owned_volume(observed, options.installation_id, options.schema_version)
                    .map_err(|ownership| ownership_conflict(options.request.name(), ownership))?;
            let desired = options.request.metadata();
            let actual = volume.metadata();
            if actual.kind() != ResourceKind::Volume
                || actual.project_id().is_some()
                || actual.retention() != RetentionClass::Persistent
                || actual.compatibility_fingerprint() != desired.compatibility_fingerprint()
            {
                return Err(SharedInfrastructureReconcileError::Conflict {
                    detail: format!(
                        "shared volume '{}' ownership does not match its compatibility identity",
                        options.request.name()
                    ),
                });
            }

            Ok(SharedVolumeReconcileResult::new(
                volume,
                SharedVolumeReconcileAction::Unchanged,
            ))
        }
        volumes => Err(SharedInfrastructureReconcileError::Conflict {
            detail: format!(
                "shared volume '{}' was observed {} times; refusing to guess",
                options.request.name(),
                volumes.len()
            ),
        }),
    }
}

fn validate_request(
    options: &SharedVolumeReconcileOptions<'_>,
) -> Result<(), SharedInfrastructureReconcileError> {
    let metadata = options.request.metadata();
    if metadata.kind() != ResourceKind::Volume
        || metadata.project_id().is_some()
        || metadata.installation_id() != options.installation_id
        || metadata.schema_version() != options.schema_version
        || metadata.retention() != RetentionClass::Persistent
    {
        return Err(SharedInfrastructureReconcileError::InvalidRequest {
            detail: "shared volume ownership does not match the active installation".to_owned(),
        });
    }

    Ok(())
}

fn ownership_conflict(
    name: &str,
    _ownership: ObservedResourceOwnership,
) -> SharedInfrastructureReconcileError {
    SharedInfrastructureReconcileError::Conflict {
        detail: format!("shared volume '{name}' exists without current-installation ownership"),
    }
}

fn engine_error(action: &str, error: EngineError) -> SharedInfrastructureReconcileError {
    SharedInfrastructureReconcileError::Engine {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}
