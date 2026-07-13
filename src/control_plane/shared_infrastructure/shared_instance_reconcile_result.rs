use crate::control_plane::engine::{ContainerHealth, ManagedResourceMetadata, RetentionClass};
use crate::control_plane::state::{
    LogicalResourceRecord, ResourceLifecycle, ResourceRecord, ResourceRecordOptions,
    ResourceRetention,
};

/// Durable ownership produced by one complete shared-instance convergence pass.
pub(crate) struct SharedInstanceReconcileResult {
    physical_resources: Vec<ResourceRecord>,
    logical_resources: Vec<LogicalResourceRecord>,
    health: ContainerHealth,
}

impl SharedInstanceReconcileResult {
    pub(crate) fn new(
        container_id: &str,
        container_metadata: &ManagedResourceMetadata,
        volume: Option<(&str, &ManagedResourceMetadata)>,
        logical_resources: Vec<LogicalResourceRecord>,
        health: ContainerHealth,
    ) -> Self {
        let mut physical_resources = vec![resource_record(container_id, container_metadata)];
        if let Some((volume_name, volume_metadata)) = volume {
            physical_resources.push(resource_record(volume_name, volume_metadata));
        }

        Self {
            physical_resources,
            logical_resources,
            health,
        }
    }

    pub(crate) fn physical_resources(&self) -> &[ResourceRecord] {
        &self.physical_resources
    }

    pub(crate) fn logical_resources(&self) -> &[LogicalResourceRecord] {
        &self.logical_resources
    }

    pub(crate) const fn health(&self) -> ContainerHealth {
        self.health
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
