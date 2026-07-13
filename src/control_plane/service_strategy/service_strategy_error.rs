use std::fmt::{Display, Formatter};

/// A preset without an explicit v8 workload-scope policy.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ServiceStrategyError {
    preset: String,
}

impl ServiceStrategyError {
    pub(super) fn unknown(preset: impl Into<String>) -> Self {
        Self {
            preset: preset.into(),
        }
    }
}

impl Display for ServiceStrategyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "unknown v8 service preset '{}'", self.preset)
    }
}

impl std::error::Error for ServiceStrategyError {}
