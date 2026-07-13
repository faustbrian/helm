use super::ProjectDiscoveryError;
use crate::control_plane::application::ControlPlaneError;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// A complete-scan discovery or atomic registry reconciliation failure.
#[derive(Debug)]
pub(crate) enum DiscoveryReconciliationError {
    Discovery(ProjectDiscoveryError),
    ControlPlane(ControlPlaneError),
}

impl Display for DiscoveryReconciliationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Discovery(error) => Display::fmt(error, formatter),
            Self::ControlPlane(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for DiscoveryReconciliationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Discovery(error) => Some(error),
            Self::ControlPlane(error) => Some(error),
        }
    }
}

impl From<ProjectDiscoveryError> for DiscoveryReconciliationError {
    fn from(error: ProjectDiscoveryError) -> Self {
        Self::Discovery(error)
    }
}

impl From<ControlPlaneError> for DiscoveryReconciliationError {
    fn from(error: ControlPlaneError) -> Self {
        Self::ControlPlane(error)
    }
}
