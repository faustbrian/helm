use std::path::{Path, PathBuf};

/// Immutable paths for one fully persisted certificate bundle revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StoredCertificatePaths {
    directory: PathBuf,
}

impl StoredCertificatePaths {
    pub(super) fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }

    pub(crate) fn ca_certificate(&self) -> PathBuf {
        self.directory.join("ca.crt")
    }

    pub(crate) fn ca_private_key(&self) -> PathBuf {
        self.directory.join("ca.key")
    }

    pub(crate) fn leaf_certificate(&self) -> PathBuf {
        self.directory.join("wildcard.crt")
    }

    pub(crate) fn leaf_private_key(&self) -> PathBuf {
        self.directory.join("wildcard.key")
    }

    pub(crate) fn renew_after(&self) -> PathBuf {
        self.directory.join("renew-after")
    }
}
