use crate::control_plane::state::{CredentialRecord, LogicalResourceRecord, ResourceRecord};
use std::time::Duration;

/// Complete durable evidence for revoking orphaned shared-service access.
pub(crate) struct OrphanedSharedAccessOptions<'operation> {
    pub(crate) resources: &'operation [ResourceRecord],
    pub(crate) logical_resources: &'operation [LogicalResourceRecord],
    pub(crate) credentials: &'operation [CredentialRecord],
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
    pub(crate) timeout: Duration,
}
