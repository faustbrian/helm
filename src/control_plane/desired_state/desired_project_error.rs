use crate::control_plane::IdentityError;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// A failure to convert raw v8 configuration into desired state.
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum DesiredProjectError {
    /// A project, service, or dependency identity is invalid.
    Identity(IdentityError),
    /// A service refers to a dependency absent from the same project.
    UnknownDependency { service: String, dependency: String },
    /// Service dependencies contain a cycle.
    DependencyCycle { cycle: Vec<String> },
    /// A service declaration is structurally or semantically incomplete.
    InvalidService { service: String, detail: String },
}

impl Display for DesiredProjectError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Identity(error) => Display::fmt(error, formatter),
            Self::UnknownDependency {
                service,
                dependency,
            } => write!(
                formatter,
                "service '{service}' depends on unknown service '{dependency}'"
            ),
            Self::DependencyCycle { cycle } => {
                write!(
                    formatter,
                    "service dependency cycle: {}",
                    cycle.join(" -> ")
                )
            }
            Self::InvalidService { service, detail } => {
                write!(formatter, "service '{service}' {detail}")
            }
        }
    }
}

impl Error for DesiredProjectError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Identity(error) => Some(error),
            Self::UnknownDependency { .. }
            | Self::DependencyCycle { .. }
            | Self::InvalidService { .. } => None,
        }
    }
}

impl From<IdentityError> for DesiredProjectError {
    fn from(error: IdentityError) -> Self {
        Self::Identity(error)
    }
}
