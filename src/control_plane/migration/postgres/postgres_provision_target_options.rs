use crate::control_plane::shared_infrastructure::PostgresLogicalResourcePlan;
use crate::control_plane::state::{CredentialRecord, LogicalResourceRecord, MigrationRecord};

/// Complete input for idempotent PostgreSQL migration target creation.
#[derive(Debug)]
pub(crate) struct PostgresProvisionTargetOptions<'operation> {
    pub(crate) checkpoint: &'operation MigrationRecord,
    pub(crate) target_logical_resource: &'operation LogicalResourceRecord,
    pub(crate) plan: &'operation PostgresLogicalResourcePlan,
    pub(crate) administrator: &'operation CredentialRecord,
    pub(crate) installation_id: &'operation str,
}
