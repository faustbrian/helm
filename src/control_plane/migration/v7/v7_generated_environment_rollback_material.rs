use std::path::{Path, PathBuf};

/// Secret-free durable reference to one private legacy environment envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct V7GeneratedEnvironmentRollbackMaterial {
    recovery_point: PathBuf,
    artifact_sha256: String,
    artifact_size_bytes: u64,
}

impl V7GeneratedEnvironmentRollbackMaterial {
    pub(crate) fn new(
        recovery_point: PathBuf,
        artifact_sha256: String,
        artifact_size_bytes: u64,
    ) -> Self {
        Self {
            recovery_point,
            artifact_sha256,
            artifact_size_bytes,
        }
    }

    pub(crate) fn recovery_point(&self) -> &Path {
        &self.recovery_point
    }

    pub(crate) fn artifact_sha256(&self) -> &str {
        &self.artifact_sha256
    }

    pub(crate) const fn artifact_size_bytes(&self) -> u64 {
        self.artifact_size_bytes
    }
}
