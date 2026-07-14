use crate::control_plane::state::{CredentialRecord, LogicalResourceRecord, ResourceRecord};

/// Complete durable evidence for revoking orphaned RabbitMQ tenant access.
pub(crate) struct OrphanedRabbitMqAccessOptions<'operation> {
    pub(crate) resources: &'operation [ResourceRecord],
    pub(crate) logical_resources: &'operation [LogicalResourceRecord],
    pub(crate) credentials: &'operation [CredentialRecord],
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
}
