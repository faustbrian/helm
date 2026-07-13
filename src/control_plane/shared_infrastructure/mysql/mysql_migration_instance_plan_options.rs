use crate::control_plane::shared_infrastructure::CredentialSecret;

/// Complete host-independent inputs for one isolated MySQL-family target.
pub(crate) struct MySqlMigrationInstancePlanOptions {
    pub(crate) migration_id: String,
    pub(crate) project_id: String,
    pub(crate) installation_id: String,
    pub(crate) network_name: String,
    pub(crate) schema_version: u32,
    pub(crate) desired_revision: String,
    pub(crate) bootstrap_secret: CredentialSecret,
}
