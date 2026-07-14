use super::MySqlFlavor;
use crate::control_plane::engine::OwnedContainer;
use crate::control_plane::state::{CredentialRecord, LogicalResourceRecord};
use std::time::Duration;

/// Exact owned MySQL-family tenant access selected for revocation.
pub(crate) struct MySqlAccessRevocationOptions<'operation> {
    pub(crate) installation_id: &'operation str,
    pub(crate) container: &'operation OwnedContainer,
    pub(crate) logical_resource: &'operation LogicalResourceRecord,
    pub(crate) credential: &'operation CredentialRecord,
    pub(crate) administrator: &'operation CredentialRecord,
    pub(crate) flavor: MySqlFlavor,
    pub(crate) timeout: Duration,
}
