use super::{ProjectVolumeReconcilePlan, WorkloadReconcileError};
use crate::control_plane::engine::{
    ObservedVolume, ResourceKind, RetentionClass, VolumeCreateOptions, reconstruct_owned_volume,
};

/// Validates ownership and plans one retained volume without Engine mutation.
pub(crate) fn plan_project_volume_reconciliation(
    request: &VolumeCreateOptions,
    observed: &[ObservedVolume],
    installation_id: &str,
    schema_version: u32,
) -> Result<ProjectVolumeReconcilePlan, WorkloadReconcileError> {
    validate_request(request, installation_id, schema_version)?;
    let matching = observed
        .iter()
        .filter(|volume| volume.name() == request.name())
        .collect::<Vec<_>>();

    match matching.as_slice() {
        [] => Ok(ProjectVolumeReconcilePlan::Create(request.clone())),
        [observed] => {
            let volume = reconstruct_owned_volume(observed, installation_id, schema_version)
                .map_err(|ownership| WorkloadReconcileError::Conflict {
                    detail: format!(
                        "project volume '{}' exists without exact current-installation ownership: {ownership:?}",
                        request.name()
                    ),
                })?;
            if volume.metadata() != request.metadata() {
                return Err(WorkloadReconcileError::DestructiveReplacementRequired {
                    detail: format!(
                        "project volume '{}' ownership differs from its requested data identity; explicit migration is required",
                        request.name()
                    ),
                });
            }

            Ok(ProjectVolumeReconcilePlan::Adopt(volume))
        }
        volumes => Err(WorkloadReconcileError::Conflict {
            detail: format!(
                "project volume '{}' was observed {} times; refusing to guess",
                request.name(),
                volumes.len()
            ),
        }),
    }
}

fn validate_request(
    request: &VolumeCreateOptions,
    installation_id: &str,
    schema_version: u32,
) -> Result<(), WorkloadReconcileError> {
    let metadata = request.metadata();
    if metadata.kind() != ResourceKind::Volume
        || metadata.project_id().is_none()
        || metadata.resource_id().is_none()
        || metadata.installation_id() != installation_id
        || metadata.schema_version() != schema_version
        || metadata.retention() != RetentionClass::Persistent
    {
        return Err(WorkloadReconcileError::InvalidRequest {
            detail: "project volume ownership does not match the active installation".to_owned(),
        });
    }

    Ok(())
}
