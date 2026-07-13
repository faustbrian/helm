use crate::control_plane::shared_infrastructure::CredentialSecret;

/// Host-independent inputs required to materialize one PostgreSQL instance.
pub(crate) struct PostgresSharedInstancePlanOptions {
    pub(crate) installation_id: String,
    pub(crate) network_name: String,
    pub(crate) schema_version: u32,
    pub(crate) desired_revision: String,
    pub(crate) bootstrap_secret: CredentialSecret,
}
