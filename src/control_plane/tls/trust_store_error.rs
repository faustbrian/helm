use std::error::Error;
use std::fmt::{Display, Formatter};

/// A failure at the operating-system certificate trust boundary.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct TrustStoreError {
    detail: String,
}

impl TrustStoreError {
    pub(crate) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for TrustStoreError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl Error for TrustStoreError {}
