use super::{LocalCertificateError, TrustStoreError};
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Failure while recovering certificate material or changing OS trust.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum LocalCaTrustError {
    Certificate(LocalCertificateError),
    TrustStore(TrustStoreError),
}

impl Display for LocalCaTrustError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Certificate(error) => Display::fmt(error, formatter),
            Self::TrustStore(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for LocalCaTrustError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Certificate(error) => Some(error),
            Self::TrustStore(error) => Some(error),
        }
    }
}

impl From<LocalCertificateError> for LocalCaTrustError {
    fn from(error: LocalCertificateError) -> Self {
        Self::Certificate(error)
    }
}

impl From<TrustStoreError> for LocalCaTrustError {
    fn from(error: TrustStoreError) -> Self {
        Self::TrustStore(error)
    }
}
