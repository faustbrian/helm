use super::LogicalResourceDrift;
use crate::control_plane::engine::{
    ContainerHealth, ManagedResourceMetadata, OwnedContainer, RetentionClass,
};
use crate::control_plane::state::{
    LogicalResourceRecord, ResourceLifecycle, ResourceRecord, ResourceRecordOptions,
    ResourceRetention,
};

/// Durable ownership produced by one complete shared-instance convergence pass.
pub(crate) struct SharedInstanceReconcileResult {
    container: OwnedContainer,
    physical_resources: Vec<ResourceRecord>,
    logical_resources: Vec<LogicalResourceRecord>,
    logical_resource_drifts: Vec<LogicalResourceDrift>,
    health: ContainerHealth,
}

impl SharedInstanceReconcileResult {
    pub(crate) fn new(
        container: OwnedContainer,
        volume: Option<(&str, &ManagedResourceMetadata)>,
        logical_resources: Vec<LogicalResourceRecord>,
        health: ContainerHealth,
    ) -> Self {
        let mut physical_resources = vec![resource_record(
            container.id().as_str(),
            container.metadata(),
        )];
        if let Some((volume_name, volume_metadata)) = volume {
            physical_resources.push(resource_record(volume_name, volume_metadata));
        }

        Self {
            container,
            physical_resources,
            logical_resources,
            logical_resource_drifts: Vec::new(),
            health,
        }
    }

    pub(crate) const fn container(&self) -> &OwnedContainer {
        &self.container
    }

    pub(crate) fn with_logical_resource_drifts(
        mut self,
        logical_resource_drifts: Vec<LogicalResourceDrift>,
    ) -> Self {
        self.logical_resource_drifts = logical_resource_drifts;

        self
    }

    pub(crate) fn physical_resources(&self) -> &[ResourceRecord] {
        &self.physical_resources
    }

    pub(crate) fn logical_resources(&self) -> &[LogicalResourceRecord] {
        &self.logical_resources
    }

    pub(crate) fn logical_resource_drifts(&self) -> &[LogicalResourceDrift] {
        &self.logical_resource_drifts
    }

    pub(crate) const fn health(&self) -> ContainerHealth {
        self.health
    }
}

fn resource_record(resource_id: &str, metadata: &ManagedResourceMetadata) -> ResourceRecord {
    let record = ResourceRecord::new(ResourceRecordOptions {
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
