use crate::control_plane::engine::{ManagedResourceMetadata, RetentionClass};
use crate::control_plane::state::{ResourceRecord, ResourceRetention};

/// Proves that an Engine label set is the exact durable physical resource.
pub(crate) fn matches_durable_resource_metadata(
    resource: &ResourceRecord,
    metadata: &ManagedResourceMetadata,
) -> bool {
    resource.installation_id() == metadata.installation_id()
        && resource.kind() == metadata.kind().label()
        && resource.scope_id() == metadata.resource_id()
        && resource.compatibility_fingerprint() == metadata.compatibility_fingerprint()
        && resource.project_id() == metadata.project_id()
        && resource.schema_version() == metadata.schema_version()
        && resource.desired_revision() == metadata.desired_revision()
        && resource.retention() == retention(metadata.retention())
}

const fn retention(retention: RetentionClass) -> ResourceRetention {
    match retention {
        RetentionClass::Persistent => ResourceRetention::Persistent,
        RetentionClass::Disposable => ResourceRetention::Disposable,
        RetentionClass::BuildCache => ResourceRetention::BuildCache,
    }
}
