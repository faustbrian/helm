use crate::control_plane::engine::OwnedContainer;
use crate::control_plane::migration::{MigrationCutoverPlan, MigrationRollbackPlan};
use crate::control_plane::shared_infrastructure::PostgresLogicalResourcePlan;
use crate::control_plane::state::{CredentialRecord, LogicalResourceRecord};
use std::path::Path;
use std::time::Duration;

/// Complete immutable context for one PostgreSQL migration execution.
#[derive(Debug)]
pub(crate) struct PostgresMigrationOperationsOptions<'operation> {
    pub(crate) source_container: &'operation OwnedContainer,
    pub(crate) target_container: &'operation OwnedContainer,
    pub(crate) source_logical_resource: &'operation LogicalResourceRecord,
    pub(crate) target_logical_resource: &'operation LogicalResourceRecord,
    pub(crate) source_credential: &'operation CredentialRecord,
    pub(crate) target_credential: &'operation CredentialRecord,
    pub(crate) target_plan: &'operation PostgresLogicalResourcePlan,
    pub(crate) administrator: &'operation CredentialRecord,
    pub(crate) installation_id: &'operation str,
    pub(crate) source_database_name: &'operation str,
    pub(crate) backup_root: &'operation Path,
    pub(crate) operation_unix_seconds: i64,
    pub(crate) timeout: Duration,
    pub(crate) cutover: MigrationCutoverPlan,
    pub(crate) rollback: MigrationRollbackPlan,
}
