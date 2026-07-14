use std::path::{Path, PathBuf};

/// Host path mounted as the singleton gateway's immutable bootstrap.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StoredGatewayBootstrapPaths {
    config_path: PathBuf,
}

impl StoredGatewayBootstrapPaths {
    pub(super) const fn new(config_path: PathBuf) -> Self {
        Self { config_path }
    }

    pub(crate) fn config_path(&self) -> &Path {
        &self.config_path
    }
}
