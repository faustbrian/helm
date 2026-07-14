use crate::control_plane::shared_infrastructure::CredentialSecret;

/// Complete host-independent inputs for one isolated SQL Server target.
pub(crate) struct SqlServerMigrationInstancePlanOptions {
    pub(crate) migration_id: String,
    pub(crate) project_id: String,
    pub(crate) installation_id: String,
    pub(crate) network_name: String,
    pub(crate) schema_version: u32,
    pub(crate) desired_revision: String,
    pub(crate) bootstrap_secret: CredentialSecret,
    pub(crate) accept_eula: bool,
    pub(crate) sqlcmd_path: String,
}
