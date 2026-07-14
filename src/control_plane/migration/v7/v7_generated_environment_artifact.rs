use std::path::{Path, PathBuf};

/// Secret-free metadata proving where generated legacy environment may exist.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct V7GeneratedEnvironmentArtifact {
    path: PathBuf,
    size_bytes: u64,
    modified_at_unix_seconds: i64,
    keys: Vec<String>,
}

impl V7GeneratedEnvironmentArtifact {
    pub(super) fn new(
        path: PathBuf,
        size_bytes: u64,
        modified_at_unix_seconds: i64,
        keys: Vec<String>,
    ) -> Self {
        Self {
            path,
            size_bytes,
            modified_at_unix_seconds,
            keys,
        }
    }

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
