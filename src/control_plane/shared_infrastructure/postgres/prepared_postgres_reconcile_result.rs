use crate::control_plane::engine::{ManagedResourceMetadata, RetentionClass};
use crate::control_plane::shared_infrastructure::SharedServiceReconcileResult;
use crate::control_plane::state::{
    LogicalResourceRecord, ResourceLifecycle, ResourceRecord, ResourceRecordOptions,
    ResourceRetention,
};

/// Durable ownership produced by one complete PostgreSQL convergence pass.
pub(crate) struct PreparedPostgresReconcileResult {
    physical_resources: Vec<ResourceRecord>,
    logical_resources: Vec<LogicalResourceRecord>,
}

impl PreparedPostgresReconcileResult {
    pub(super) fn new(
        shared: SharedServiceReconcileResult,
        logical_resources: Vec<LogicalResourceRecord>,
    ) -> Self {
        let mut physical_resources = vec![resource_record(
            shared.container().id().as_str(),
            shared.container().metadata(),
        )];
        if let Some(volume) = shared.volume() {
            physical_resources.push(resource_record(
                volume.volume().name(),
                volume.volume().metadata(),
            ));
        }

        Self {
            physical_resources,
            logical_resources,
        }
    }

    pub(crate) fn physical_resources(&self) -> &[ResourceRecord] {
        &self.physical_resources
    }

    pub(crate) fn logical_resources(&self) -> &[LogicalResourceRecord] {
        &self.logical_resources
    }
}

fn resource_record(resource_id: &str, metadata: &ManagedResourceMetadata) -> ResourceRecord {
    ResourceRecord::new(ResourceRecordOptions {
        resource_id: resource_id.to_owned(),
        installation_id: metadata.installation_id().to_owned(),
        kind: metadata.kind().label().to_owned(),
        compatibility_fingerprint: metadata.compatibility_fingerprint().to_owned(),
        project_id: metadata.project_id().map(str::to_owned),
        schema_version: metadata.schema_version(),
        desired_revision: metadata.desired_revision().to_owned(),
        retention: retention(metadata.retention()),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
}

const fn retention(retention: RetentionClass) -> ResourceRetention {
    match retention {
        RetentionClass::Persistent => ResourceRetention::Persistent,
        RetentionClass::Disposable => ResourceRetention::Disposable,
        RetentionClass::BuildCache => ResourceRetention::BuildCache,
    }
}
