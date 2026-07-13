use std::path::{Path, PathBuf};

/// Host paths mounted into and used to control the singleton gateway.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StoredGatewayBootstrapPaths {
    config_path: PathBuf,
    runtime_directory: PathBuf,
    admin_socket_path: PathBuf,
}

impl StoredGatewayBootstrapPaths {
    pub(super) fn new(config_path: PathBuf, runtime_directory: PathBuf) -> Self {
        let admin_socket_path = runtime_directory.join("admin.sock");

        Self {
            config_path,
            runtime_directory,
            admin_socket_path,
        }
    }

    pub(crate) fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub(crate) fn runtime_directory(&self) -> &Path {
        &self.runtime_directory
    }

    pub(crate) fn admin_socket_path(&self) -> &Path {
        &self.admin_socket_path
    }
}
