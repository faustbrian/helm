use crate::control_plane::engine::OwnedContainer;
use crate::control_plane::shared_infrastructure::RedisFlavor;
use crate::control_plane::state::{CredentialRecord, LogicalResourceRecord};
use std::time::Duration;

/// Exact runtime inputs for deleting one confirmed Redis-compatible tenant.
pub(crate) struct RedisLogicalPruneOptions<'operation> {
    pub(crate) installation_id: &'operation str,
    pub(crate) flavor: RedisFlavor,
    pub(crate) container: &'operation OwnedContainer,
    pub(crate) logical_resource: &'operation LogicalResourceRecord,
    pub(crate) credential: &'operation CredentialRecord,
    pub(crate) administrator: &'operation CredentialRecord,
    pub(crate) timeout: Duration,
}
