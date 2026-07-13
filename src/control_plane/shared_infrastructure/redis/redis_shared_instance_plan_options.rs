use crate::control_plane::shared_infrastructure::CredentialSecret;
use std::path::PathBuf;

/// Inputs required to materialize one Redis-compatible shared instance.
pub(crate) struct RedisSharedInstancePlanOptions {
    pub(crate) installation_id: String,
    pub(crate) network_name: String,
    pub(crate) schema_version: u32,
    pub(crate) desired_revision: String,
    pub(crate) acl_directory: PathBuf,
    pub(crate) bootstrap_secret: CredentialSecret,
}
