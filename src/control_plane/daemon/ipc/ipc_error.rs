use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

/// A local IPC framing, protocol, or endpoint failure.
#[derive(Debug)]
#[non_exhaustive]
pub(crate) enum IpcError {
    /// JSON serialization or strict deserialization failed.
    Json(serde_json::Error),
    /// A frame is not terminated by exactly one newline.
    InvalidFrame,
    /// A frame exceeds the bounded local protocol size.
    FrameTooLarge { actual: usize, maximum: usize },
    /// The client and daemon protocol versions do not match.
    UnsupportedProtocol { found: u16, expected: u16 },
    /// A request cannot be correlated or cancelled without an ID.
    EmptyRequestId,
    /// A client timeout must bound both request and response I/O.
    InvalidTimeout,
    /// A response must complete the exact request sent on the connection.
    ResponseCorrelation { expected: String, found: String },
    /// A local endpoint could not be bound or secured.
    EndpointIo {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl Display for IpcError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid IPC JSON: {error}"),
            Self::InvalidFrame => write!(
                formatter,
                "IPC frame must contain one newline-terminated request"
            ),
            Self::FrameTooLarge { actual, maximum } => write!(
                formatter,
                "IPC frame is {actual} bytes; maximum is {maximum} bytes"
            ),
            Self::UnsupportedProtocol { found, expected } => write!(
                formatter,
                "IPC protocol version {found} is unsupported; expected {expected}"
            ),
            Self::EmptyRequestId => write!(formatter, "IPC request_id must not be empty"),
            Self::InvalidTimeout => write!(formatter, "IPC timeout must be greater than zero"),
            Self::ResponseCorrelation { expected, found } => write!(
                formatter,
                "IPC response request_id '{found}' does not match request '{expected}'"
            ),
            Self::EndpointIo { path, source } => write!(
                formatter,
                "IPC endpoint '{}' failed: {source}",
                path.display()
            ),
        }
    }
}

impl Error for IpcError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::EndpointIo { source, .. } => Some(source),
            Self::InvalidFrame
            | Self::FrameTooLarge { .. }
            | Self::UnsupportedProtocol { .. }
            | Self::EmptyRequestId
            | Self::InvalidTimeout
            | Self::ResponseCorrelation { .. } => None,
        }
    }
}

impl From<serde_json::Error> for IpcError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}
