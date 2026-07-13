use std::fmt::{Display, Formatter};

/// Invalid identity or timestamp rejected before publishing live health.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResourceHealthRegistryError {
    EmptyResourceId,
    NegativeObservationTime,
}

impl Display for ResourceHealthRegistryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyResourceId => formatter.write_str("health resource ID must not be empty"),
            Self::NegativeObservationTime => {
                formatter.write_str("health observation time must not be negative")
            }
        }
    }
}

impl std::error::Error for ResourceHealthRegistryError {}
