use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Secret-free metadata for a legacy generated environment file.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcV7GeneratedEnvironmentArtifact {
    path: PathBuf,
    size_bytes: u64,
    modified_at_unix_seconds: i64,
    keys: Vec<String>,
}

impl From<&crate::control_plane::migration::V7GeneratedEnvironmentArtifact>
    for IpcV7GeneratedEnvironmentArtifact
{
    fn from(artifact: &crate::control_plane::migration::V7GeneratedEnvironmentArtifact) -> Self {
        Self {
            path: artifact.path().to_path_buf(),
            size_bytes: artifact.size_bytes(),
            modified_at_unix_seconds: artifact.modified_at_unix_seconds(),
            keys: artifact.keys().to_vec(),
        }
    }
}

impl IpcV7GeneratedEnvironmentArtifact {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) const fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    pub(crate) const fn modified_at_unix_seconds(&self) -> i64 {
        self.modified_at_unix_seconds
    }

    pub(crate) fn keys(&self) -> &[String] {
        &self.keys
    }
}
