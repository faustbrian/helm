use crate::control_plane::state::{CredentialRecord, MigrationRecord};
use std::time::Duration;

/// Complete bounded input for authenticated MongoDB target verification.
pub(crate) struct MongoDbVerifyTargetOptions<'operation> {
    pub(crate) checkpoint: &'operation MigrationRecord,
    pub(crate) credential: &'operation CredentialRecord,
    pub(crate) installation_id: &'operation str,
    pub(crate) target_database_name: &'operation str,
    pub(crate) timeout: Duration,
}
