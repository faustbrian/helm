use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

/// A malformed, stale, or incomplete project-local artifact lock.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ArtifactLockError {
    path: PathBuf,
    detail: String,
}

impl ArtifactLockError {
    pub(crate) fn new(path: PathBuf, detail: impl Into<String>) -> Self {
        Self {
            path,
            detail: detail.into(),
        }
    }
}

impl Display for ArtifactLockError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid v8 artifact lock '{}': {}",
            self.path.display(),
            self.detail
        )
    }
}

impl Error for ArtifactLockError {}
