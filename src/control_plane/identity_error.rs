use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

/// An invalid v8 project, service, or route identity.
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum IdentityError {
    /// A supplied project or service name is not already a DNS label.
    InvalidName { kind: &'static str, value: String },
    /// A project directory does not have a usable UTF-8 basename.
    MissingDirectoryBasename { path: PathBuf },
    /// The project and service names form an overlong DNS label.
    RouteLabelTooLong { label: String },
}

impl Display for IdentityError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName { kind, value } => write!(
                formatter,
                "{kind} name '{value}' must be a valid lowercase DNS label"
            ),
            Self::MissingDirectoryBasename { path } => write!(
                formatter,
                "project directory '{}' must have a UTF-8 basename",
                path.display()
            ),
            Self::RouteLabelTooLong { label } => {
                write!(formatter, "route label '{label}' exceeds 63 bytes")
            }
        }
    }
}

impl Error for IdentityError {}
