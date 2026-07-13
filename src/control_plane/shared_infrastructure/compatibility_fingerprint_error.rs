use std::error::Error;
use std::fmt::{Display, Formatter};

/// A compatibility profile that cannot safely identify a shared instance.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct CompatibilityFingerprintError {
    detail: String,
}

impl CompatibilityFingerprintError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for CompatibilityFingerprintError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl Error for CompatibilityFingerprintError {}
