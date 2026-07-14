use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Exact public legacy artifact safe to expose over user-only IPC.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcV7PublicFileArtifact {
    path: PathBuf,
    revision: String,
    size_bytes: u64,
}

impl From<&crate::control_plane::migration::V7PublicFileArtifact> for IpcV7PublicFileArtifact {
    fn from(artifact: &crate::control_plane::migration::V7PublicFileArtifact) -> Self {
        Self {
            path: artifact.path().to_path_buf(),
            revision: artifact.revision().to_owned(),
            size_bytes: artifact.size_bytes(),
        }
    }
}

impl IpcV7PublicFileArtifact {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn revision(&self) -> &str {
        &self.revision
    }

    pub(crate) const fn size_bytes(&self) -> u64 {
        self.size_bytes
    }
}
