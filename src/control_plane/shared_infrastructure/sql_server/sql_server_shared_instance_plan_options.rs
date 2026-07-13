use crate::control_plane::shared_infrastructure::CredentialSecret;

/// Inputs required to materialize one shared SQL Server instance.
pub(crate) struct SqlServerSharedInstancePlanOptions {
    pub(crate) installation_id: String,
    pub(crate) network_name: String,
    pub(crate) schema_version: u32,
    pub(crate) desired_revision: String,
    pub(crate) bootstrap_secret: CredentialSecret,
    pub(crate) accept_eula: bool,
    pub(crate) sqlcmd_path: String,
}
