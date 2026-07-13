use super::RegistryPlanError;
use crate::control_plane::state::StateStoreError;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// A control-plane planning or durable transaction failure.
#[derive(Debug)]
#[non_exhaustive]
pub(crate) enum ControlPlaneError {
    Plan(RegistryPlanError),
    State(StateStoreError),
}

impl Display for ControlPlaneError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plan(error) => Display::fmt(error, formatter),
            Self::State(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for ControlPlaneError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Plan(error) => Some(error),
            Self::State(error) => Some(error),
        }
    }
}

impl From<RegistryPlanError> for ControlPlaneError {
    fn from(error: RegistryPlanError) -> Self {
        Self::Plan(error)
    }
}

impl From<StateStoreError> for ControlPlaneError {
    fn from(error: StateStoreError) -> Self {
        Self::State(error)
    }
}
