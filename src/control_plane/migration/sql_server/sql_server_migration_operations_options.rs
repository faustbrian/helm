use crate::control_plane::engine::OwnedContainer;
use crate::control_plane::migration::{MigrationCutoverPlan, MigrationRollbackPlan};
use crate::control_plane::shared_infrastructure::{
    SqlServerLogicalResourcePlan, SqlServerSharedInstancePlan,
};
use crate::control_plane::state::{
    CredentialRecord, LogicalResourceRecord, ManagedEnvironmentRecord,
};
use std::path::Path;
use std::time::Duration;

/// Complete immutable context for one SQL Server migration execution.
pub(crate) struct SqlServerMigrationOperationsOptions<'operation> {
    pub(crate) source_container: &'operation OwnedContainer,
    pub(crate) target_container: &'operation OwnedContainer,
    pub(crate) source_logical_resource: &'operation LogicalResourceRecord,
    pub(crate) target_logical_resource: &'operation LogicalResourceRecord,
    pub(crate) source_credential: &'operation CredentialRecord,
    pub(crate) target_credential: &'operation CredentialRecord,
    pub(crate) source_administrator: &'operation CredentialRecord,
    pub(crate) target_instance: &'operation SqlServerSharedInstancePlan,
    pub(crate) source_environment: &'operation ManagedEnvironmentRecord,
    pub(crate) target_plan: &'operation SqlServerLogicalResourcePlan,
    pub(crate) installation_id: &'operation str,
    pub(crate) backup_root: &'operation Path,
    pub(crate) operation_unix_seconds: i64,
    pub(crate) timeout: Duration,
    pub(crate) cutover: MigrationCutoverPlan,
    pub(crate) rollback: MigrationRollbackPlan,
}
