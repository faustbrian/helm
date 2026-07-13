use crate::control_plane::configuration::{ArtifactLockError, ConfigParseError};
use crate::control_plane::{DesiredProjectError, RegistryConflicts};
use std::error::Error;
use std::fmt::{Display, Formatter};

/// A complete-registry parse, desired-state, or ownership failure.
#[derive(Debug)]
#[non_exhaustive]
pub(crate) enum RegistryPlanError {
    Configuration(ConfigParseError),
    ArtifactLock(ArtifactLockError),
    DesiredProject(DesiredProjectError),
    RouteOwnership(RegistryConflicts),
}

impl Display for RegistryPlanError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Configuration(error) => Display::fmt(error, formatter),
            Self::ArtifactLock(error) => Display::fmt(error, formatter),
            Self::DesiredProject(error) => Display::fmt(error, formatter),
            Self::RouteOwnership(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for RegistryPlanError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Configuration(error) => Some(error),
            Self::ArtifactLock(error) => Some(error),
            Self::DesiredProject(error) => Some(error),
            Self::RouteOwnership(error) => Some(error),
        }
    }
}

impl From<ArtifactLockError> for RegistryPlanError {
    fn from(error: ArtifactLockError) -> Self {
        Self::ArtifactLock(error)
    }
}

impl From<ConfigParseError> for RegistryPlanError {
    fn from(error: ConfigParseError) -> Self {
        Self::Configuration(error)
    }
}

impl From<DesiredProjectError> for RegistryPlanError {
    fn from(error: DesiredProjectError) -> Self {
        Self::DesiredProject(error)
    }
}

impl From<RegistryConflicts> for RegistryPlanError {
    fn from(error: RegistryConflicts) -> Self {
        Self::RouteOwnership(error)
    }
}
