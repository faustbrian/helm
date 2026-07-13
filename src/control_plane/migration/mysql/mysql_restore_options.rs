use crate::control_plane::shared_infrastructure::MySqlFlavor;
use crate::control_plane::state::{CredentialRecord, LogicalResourceRecord, MigrationRecord};
use std::time::Duration;

/// Complete bounded input for one verified MySQL-family restore operation.
#[derive(Debug)]
pub(crate) struct MySqlRestoreOptions<'operation> {
    pub(crate) flavor: MySqlFlavor,
    pub(crate) checkpoint: &'operation MigrationRecord,
    pub(crate) source_logical_resource: &'operation LogicalResourceRecord,
    pub(crate) credential: &'operation CredentialRecord,
    pub(crate) installation_id: &'operation str,
    pub(crate) target_database_name: &'operation str,
    pub(crate) verified_at_unix_seconds: i64,
    pub(crate) timeout: Duration,
}
