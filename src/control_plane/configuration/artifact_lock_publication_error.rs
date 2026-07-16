use std::error::Error;
use std::fmt::{Display, Formatter};

/// A fail-closed artifact-lock filesystem publication failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ArtifactLockPublicationError {
    detail: String,
}

impl ArtifactLockPublicationError {
    pub(crate) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for ArtifactLockPublicationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl Error for ArtifactLockPublicationError {}
