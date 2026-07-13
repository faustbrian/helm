use std::fmt::{Display, Formatter};

/// A preset/version pair without an exact built-in artifact contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PresetArtifactError {
    detail: String,
}

impl PresetArtifactError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for PresetArtifactError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for PresetArtifactError {}
