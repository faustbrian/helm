use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ManagedSecretStoreError {
    detail: String,
}

impl ManagedSecretStoreError {
    pub(crate) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for ManagedSecretStoreError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl Error for ManagedSecretStoreError {}
