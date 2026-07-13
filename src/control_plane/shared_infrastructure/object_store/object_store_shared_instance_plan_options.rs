use crate::control_plane::shared_infrastructure::CredentialSecret;
use std::path::PathBuf;

/// Inputs required to materialize one shared object-store instance.
pub(crate) struct ObjectStoreSharedInstancePlanOptions {
    pub(crate) installation_id: String,
    pub(crate) network_name: String,
    pub(crate) schema_version: u32,
    pub(crate) desired_revision: String,
    pub(crate) policy_directory: PathBuf,
    pub(crate) root_secret: CredentialSecret,
}
