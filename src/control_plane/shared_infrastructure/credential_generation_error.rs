use std::error::Error;
use std::fmt::{Display, Formatter};

/// Failure to obtain cryptographically secure managed-credential entropy.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct CredentialGenerationError {
    detail: String,
}

impl CredentialGenerationError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for CredentialGenerationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl Error for CredentialGenerationError {}
