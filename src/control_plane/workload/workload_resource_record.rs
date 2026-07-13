use super::WorkloadReconcileResult;
use crate::control_plane::engine::RetentionClass;
use crate::control_plane::state::{
    ResourceLifecycle, ResourceRecord, ResourceRecordOptions, ResourceRetention,
};

/// Converts observed Engine ownership into one durable project workload record.
pub(crate) fn workload_resource_record(result: &WorkloadReconcileResult) -> ResourceRecord {
    let container = result.container();
    let metadata = container.metadata();
    let record = ResourceRecord::new(ResourceRecordOptions {
        resource_id: container.id().as_str().to_owned(),
        installation_id: metadata.installation_id().to_owned(),
        kind: metadata.kind().label().to_owned(),
        compatibility_fingerprint: metadata.compatibility_fingerprint().to_owned(),
        project_id: metadata.project_id().map(str::to_owned),
        schema_version: metadata.schema_version(),
        desired_revision: metadata.desired_revision().to_owned(),
        retention: retention(metadata.retention()),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });

    match metadata.resource_id() {
        Some(scope_id) => record.with_scope_id(scope_id),
        None => record,
    }
}

const fn retention(retention: RetentionClass) -> ResourceRetention {
    match retention {
        RetentionClass::Persistent => ResourceRetention::Persistent,
        RetentionClass::Disposable => ResourceRetention::Disposable,
        RetentionClass::BuildCache => ResourceRetention::BuildCache,
    }
}
