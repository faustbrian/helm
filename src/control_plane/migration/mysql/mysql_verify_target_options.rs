use crate::control_plane::shared_infrastructure::MySqlFlavor;
use crate::control_plane::state::{CredentialRecord, MigrationRecord};
use std::time::Duration;

/// Complete bounded input for authenticated MySQL-family target verification.
pub(crate) struct MySqlVerifyTargetOptions<'operation> {
    pub(crate) flavor: MySqlFlavor,
    pub(crate) checkpoint: &'operation MigrationRecord,
    pub(crate) credential: &'operation CredentialRecord,
    pub(crate) installation_id: &'operation str,
    pub(crate) target_database_name: &'operation str,
    pub(crate) timeout: Duration,
}
