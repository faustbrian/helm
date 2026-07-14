use std::path::{Path, PathBuf};

/// Exact digest for one public, non-secret legacy filesystem artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct V7PublicFileArtifact {
    path: PathBuf,
    revision: String,
    size_bytes: u64,
}

impl V7PublicFileArtifact {
    pub(crate) fn new(path: PathBuf, revision: String, size_bytes: u64) -> Self {
        Self {
            path,
            revision,
            size_bytes,
        }
    }

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
