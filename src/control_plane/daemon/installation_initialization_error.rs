use crate::control_plane::state::StateStoreError;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Failure to establish the immutable per-user installation contract.
#[derive(Debug)]
pub(crate) enum InstallationInitializationError {
    Entropy { detail: String },
    Environment { detail: String },
    State(StateStoreError),
}

impl InstallationInitializationError {
    pub(super) fn entropy(error: getrandom::Error) -> Self {
        Self::Entropy {
            detail: error.to_string(),
        }
    }

    pub(super) fn environment(detail: impl Into<String>) -> Self {
        Self::Environment {
            detail: detail.into(),
        }
    }
}

impl Display for InstallationInitializationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Entropy { detail } => write!(
                formatter,
                "failed to generate Stackctl installation identity: {detail}"
            ),
            Self::Environment { detail } => {
                write!(
                    formatter,
                    "cannot select the Docker Engine endpoint: {detail}"
                )
            }
            Self::State(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for InstallationInitializationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::State(error) => Some(error),
            Self::Entropy { .. } | Self::Environment { .. } => None,
        }
    }
}

impl From<StateStoreError> for InstallationInitializationError {
    fn from(error: StateStoreError) -> Self {
        Self::State(error)
    }
}
