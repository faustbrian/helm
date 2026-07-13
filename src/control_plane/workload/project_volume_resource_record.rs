use super::ProjectVolumeReconcileResult;
use crate::control_plane::state::{
    ResourceLifecycle, ResourceRecord, ResourceRecordOptions, ResourceRetention,
};

/// Converts one proven retained project volume into durable ownership state.
pub(crate) fn project_volume_resource_record(
    result: &ProjectVolumeReconcileResult,
) -> ResourceRecord {
    let volume = result.volume();
    let metadata = volume.metadata();

    ResourceRecord::new(ResourceRecordOptions {
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
    .with_scope_id(
        metadata
            .resource_id()
            .expect("validated project volume identity"),
    )
}
