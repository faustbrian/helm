use crate::control_plane::engine::OwnedContainer;
use crate::control_plane::state::{CredentialRecord, LogicalResourceRecord};
use std::time::Duration;

/// Exact runtime-only inputs for one PostgreSQL tenant deletion.
pub(crate) struct PostgresLogicalPruneOptions<'operation> {
    pub(crate) installation_id: &'operation str,
    pub(crate) container: &'operation OwnedContainer,
    pub(crate) logical_resource: &'operation LogicalResourceRecord,
    pub(crate) credential: &'operation CredentialRecord,
    pub(crate) administrator: &'operation CredentialRecord,
    pub(crate) timeout: Duration,
}
