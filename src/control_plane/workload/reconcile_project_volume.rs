use super::{
    ProjectVolumeReconcileAction, ProjectVolumeReconcileOptions, ProjectVolumeReconcileResult,
    WorkloadReconcileError,
};
use crate::control_plane::engine::{
    EngineError, ResourceKind, RetentionClass, VolumeDiscovery, VolumeManager,
    reconstruct_owned_volume,
};

/// Creates or adopts one exact retained project volume without deleting data.
pub(crate) async fn reconcile_project_volume<E>(
    engine: &mut E,
    options: ProjectVolumeReconcileOptions<'_>,
) -> Result<ProjectVolumeReconcileResult, WorkloadReconcileError>
where
    E: VolumeDiscovery + VolumeManager,
{
    validate_request(&options)?;
    let observed = engine
        .discover_managed_volumes()
        .await
        .map_err(|error| engine_error("discover project volumes", error))?;
    let matching = observed
        .iter()
        .filter(|volume| volume.name() == options.request.name())
        .collect::<Vec<_>>();

    match matching.as_slice() {
        [] => {
            let volume = engine
                .create_volume(options.request)
                .await
                .map_err(|error| engine_error("create project volume", error))?;
            if volume.name() != options.request.name()
                || volume.metadata() != options.request.metadata()
            {
                return Err(WorkloadReconcileError::Engine {
                    action: "create project volume".to_owned(),
                    detail: "Engine returned a project volume with unexpected ownership".to_owned(),
                });
            }

            Ok(ProjectVolumeReconcileResult::new(
                volume,
                ProjectVolumeReconcileAction::Created,
            ))
        }
        [observed] => {
            let volume = reconstruct_owned_volume(
                observed,
                options.installation_id,
                options.schema_version,
            )
            .map_err(|ownership| WorkloadReconcileError::Conflict {
                detail: format!(
                    "project volume '{}' exists without exact current-installation ownership: {ownership:?}",
                    options.request.name()
                ),
            })?;
            if volume.metadata() != options.request.metadata() {
                return Err(WorkloadReconcileError::Conflict {
                    detail: format!(
                        "project volume '{}' ownership differs from its requested data identity; explicit migration is required",
                        options.request.name()
                    ),
                });
            }

            Ok(ProjectVolumeReconcileResult::new(
                volume,
                ProjectVolumeReconcileAction::Unchanged,
            ))
        }
        volumes => Err(WorkloadReconcileError::Conflict {
            detail: format!(
                "project volume '{}' was observed {} times; refusing to guess",
                options.request.name(),
                volumes.len()
            ),
        }),
    }
}

fn validate_request(
    options: &ProjectVolumeReconcileOptions<'_>,
) -> Result<(), WorkloadReconcileError> {
    let metadata = options.request.metadata();
    if metadata.kind() != ResourceKind::Volume
        || metadata.project_id().is_none()
        || metadata.resource_id().is_none()
        || metadata.installation_id() != options.installation_id
        || metadata.schema_version() != options.schema_version
        || metadata.retention() != RetentionClass::Persistent
    {
        return Err(WorkloadReconcileError::InvalidRequest {
            detail: "project volume ownership does not match the active installation".to_owned(),
        });
    }

    Ok(())
}

fn engine_error(action: &str, error: EngineError) -> WorkloadReconcileError {
    WorkloadReconcileError::Engine {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}
