use std::path::{Path, PathBuf};

/// Host paths for one immutable artifact and its portable manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StoredBackupArtifact {
    recovery_point: PathBuf,
    artifact_file: PathBuf,
    manifest_file: PathBuf,
}

impl StoredBackupArtifact {
    pub(super) fn new(directory: &Path) -> Self {
        Self {
            recovery_point: directory.to_path_buf(),
            artifact_file: directory.join("artifact.bin"),
            manifest_file: directory.join("manifest.json"),
        }
    }

    pub(crate) fn artifact_file(&self) -> &Path {
        &self.artifact_file
    }

    pub(crate) fn manifest_file(&self) -> &Path {
        &self.manifest_file
    }

    pub(crate) fn recovery_point(&self) -> &Path {
        &self.recovery_point
    }
}
