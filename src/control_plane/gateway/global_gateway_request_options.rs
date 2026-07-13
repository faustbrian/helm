use std::path::PathBuf;

/// Complete host paths and ownership needed for the singleton gateway request.
pub(crate) struct GlobalGatewayRequestOptions {
    pub(crate) installation_id: String,
    pub(crate) container_user: String,
    pub(crate) certificate_path: PathBuf,
    pub(crate) private_key_path: PathBuf,
    pub(crate) certificate_revision: String,
    pub(crate) bootstrap_config_path: PathBuf,
    pub(crate) admin_runtime_directory: PathBuf,
}
