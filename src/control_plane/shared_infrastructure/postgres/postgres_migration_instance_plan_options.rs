use crate::control_plane::shared_infrastructure::CredentialSecret;

/// Complete host-independent inputs for one isolated PostgreSQL migration target.
pub(crate) struct PostgresMigrationInstancePlanOptions {
    pub(crate) migration_id: String,
    pub(crate) project_id: String,
    pub(crate) installation_id: String,
    pub(crate) network_name: String,
    pub(crate) schema_version: u32,
    pub(crate) desired_revision: String,
    pub(crate) bootstrap_secret: CredentialSecret,
}
