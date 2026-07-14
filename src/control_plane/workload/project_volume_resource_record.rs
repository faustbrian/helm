use super::{ProjectVolumeReconcileResult, WorkloadReconcileError};
use crate::control_plane::state::{
    ResourceLifecycle, ResourceRecord, ResourceRecordOptions, ResourceRetention,
};

/// Converts one proven retained project volume into durable ownership state.
pub(crate) fn project_volume_resource_record(
    result: &ProjectVolumeReconcileResult,
) -> Result<ResourceRecord, WorkloadReconcileError> {
    let volume = result.volume();
    let metadata = volume.metadata();
    let scope_id =
        metadata
            .resource_id()
            .ok_or_else(|| WorkloadReconcileError::InvalidRequest {
                detail: "project volume ownership metadata has no resource identity".to_owned(),
            })?;

    Ok(ResourceRecord::new(ResourceRecordOptions {
        resource_id: volume.name().to_owned(),
        installation_id: metadata.installation_id().to_owned(),
        kind: metadata.kind().label().to_owned(),
        compatibility_fingerprint: metadata.compatibility_fingerprint().to_owned(),
        project_id: metadata.project_id().map(str::to_owned),
        schema_version: metadata.schema_version(),
        desired_revision: metadata.desired_revision().to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
    .with_scope_id(scope_id))
}
