use super::ManagedResourceMetadata;
use std::path::PathBuf;

/// Complete host and Engine inputs for the singleton gateway container.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GatewayContainerRequestOptions {
    pub(super) image: String,
    pub(super) network: String,
    pub(super) user: String,
    pub(super) certificate_path: PathBuf,
    pub(super) private_key_path: PathBuf,
    pub(super) bootstrap_config_path: PathBuf,
    pub(super) admin_runtime_directory: PathBuf,
    pub(super) metadata: ManagedResourceMetadata,
}

impl GatewayContainerRequestOptions {
    pub(crate) const fn new(
        image: String,
        network: String,
        user: String,
        certificate_path: PathBuf,
        private_key_path: PathBuf,
        bootstrap_config_path: PathBuf,
        admin_runtime_directory: PathBuf,
        metadata: ManagedResourceMetadata,
    ) -> Self {
        Self {
            image,
            network,
            user,
            certificate_path,
            private_key_path,
            bootstrap_config_path,
            admin_runtime_directory,
            metadata,
        }
    }
}
