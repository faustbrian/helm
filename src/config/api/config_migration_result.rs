use super::MigrationDifference;
use std::path::{Path, PathBuf};

/// Published candidate and semantic-difference report paths.
#[derive(Debug)]
pub struct ConfigMigrationResult {
    source: PathBuf,
    candidate: PathBuf,
    report: PathBuf,
    differences: Vec<MigrationDifference>,
}

impl ConfigMigrationResult {
    pub(super) fn new(
        source: PathBuf,
        candidate: PathBuf,
        report: PathBuf,
        differences: Vec<MigrationDifference>,
    ) -> Self {
        Self {
            source,
            candidate,
            report,
            differences,
        }
    }

    pub fn source(&self) -> &Path {
        &self.source
    }

    pub fn candidate(&self) -> &Path {
        &self.candidate
    }

    pub fn report(&self) -> &Path {
        &self.report
    }

    pub fn differences(&self) -> &[MigrationDifference] {
        &self.differences
    }

    pub fn has_blocking_differences(&self) -> bool {
        self.differences.iter().any(MigrationDifference::blocking)
    }
}
