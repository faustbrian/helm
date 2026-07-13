use std::path::PathBuf;

/// Complete host paths and ownership needed for the singleton gateway request.
pub(crate) struct GlobalGatewayRequestOptions {
    pub(crate) installation_id: String,
    pub(crate) tls_directory: PathBuf,
    pub(crate) bootstrap_config_path: PathBuf,
    pub(crate) admin_runtime_directory: PathBuf,
}
