use crate::control_plane::state::{CredentialRecord, MigrationRecord};
use std::time::Duration;

/// Exact tenant identity and checkpoint required for target verification.
pub(crate) struct SqlServerVerifyTargetOptions<'operation> {
    pub(crate) checkpoint: &'operation MigrationRecord,
    pub(crate) credential: &'operation CredentialRecord,
    pub(crate) installation_id: &'operation str,
    pub(crate) target_database_name: &'operation str,
    pub(crate) timeout: Duration,
}
