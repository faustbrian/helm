use crate::control_plane::engine::OwnedContainer;
use crate::control_plane::state::{CredentialRecord, ManagedEnvironmentRecord};
use std::time::Duration;

/// Complete bounded input for confirmed PostgreSQL source retirement.
#[derive(Debug)]
pub(crate) struct PostgresSourceRetirementOptions<'operation> {
    pub(crate) source_container: &'operation OwnedContainer,
    pub(crate) administrator: &'operation CredentialRecord,
    pub(crate) source_credential: &'operation CredentialRecord,
    pub(crate) source_environment: &'operation ManagedEnvironmentRecord,
    pub(crate) installation_id: &'operation str,
    pub(crate) timeout: Duration,
}
