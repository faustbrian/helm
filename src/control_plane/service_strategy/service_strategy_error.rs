use std::fmt::{Display, Formatter};

/// Invalid preset strategy or desired execution-plan relationship.
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum ServiceStrategyError {
    UnknownPreset { preset: String },
    InvalidPlan { detail: String },
}

impl ServiceStrategyError {
    pub(super) fn unknown(preset: impl Into<String>) -> Self {
        Self::UnknownPreset {
            preset: preset.into(),
        }
    }

    pub(crate) fn invalid_plan(detail: impl Into<String>) -> Self {
        Self::InvalidPlan {
            detail: detail.into(),
        }
    }
}

impl Display for ServiceStrategyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownPreset { preset } => {
                write!(formatter, "unknown v8 service preset '{preset}'")
            }
            Self::InvalidPlan { detail } => formatter.write_str(detail),
        }
    }
}

impl std::error::Error for ServiceStrategyError {}
