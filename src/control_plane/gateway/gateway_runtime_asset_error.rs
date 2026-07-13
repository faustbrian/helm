use super::GatewayError;
use crate::control_plane::engine::EngineError;
use crate::control_plane::tls::LocalCertificateError;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Structured failure before any gateway Engine mutation is attempted.
#[derive(Debug)]
pub(crate) enum GatewayRuntimeAssetError {
    Certificate(LocalCertificateError),
    Gateway(GatewayError),
    Request(EngineError),
}

impl Display for GatewayRuntimeAssetError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Certificate(error) => {
                write!(formatter, "gateway TLS preparation failed: {error}")
            }
            Self::Gateway(error) => Display::fmt(error, formatter),
            Self::Request(error) => write!(formatter, "gateway Engine request is invalid: {error}"),
        }
    }
}

impl Error for GatewayRuntimeAssetError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Certificate(error) => Some(error),
            Self::Gateway(error) => Some(error),
            Self::Request(error) => Some(error),
        }
    }
}

impl From<LocalCertificateError> for GatewayRuntimeAssetError {
    fn from(error: LocalCertificateError) -> Self {
        Self::Certificate(error)
    }
}

impl From<GatewayError> for GatewayRuntimeAssetError {
    fn from(error: GatewayError) -> Self {
        Self::Gateway(error)
    }
}

impl From<EngineError> for GatewayRuntimeAssetError {
    fn from(error: EngineError) -> Self {
        Self::Request(error)
    }
}
