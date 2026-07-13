use std::error::Error;
use std::fmt::{Display, Formatter};

/// A failure to generate or calculate Stackctl-owned local TLS material.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct LocalCertificateError {
    detail: String,
}

impl LocalCertificateError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for LocalCertificateError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl Error for LocalCertificateError {}
