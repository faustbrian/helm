use crate::control_plane::state::{CredentialRecord, MigrationRecord};
use std::time::Duration;

/// Complete bounded input for PostgreSQL target catalog verification.
#[derive(Debug)]
pub(crate) struct PostgresVerifyTargetOptions<'operation> {
    pub(crate) checkpoint: &'operation MigrationRecord,
    pub(crate) credential: &'operation CredentialRecord,
    pub(crate) installation_id: &'operation str,
    pub(crate) target_database_name: &'operation str,
    pub(crate) target_role_name: &'operation str,
    pub(crate) timeout: Duration,
}
