/// One versioned mutable registry source selected by the built-in catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PresetArtifact {
    version: String,
    reference: String,
}

impl PresetArtifact {
    pub(super) fn new(version: impl Into<String>, reference: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            reference: reference.into(),
        }
    }

    pub(crate) fn version(&self) -> &str {
        &self.version
    }

    pub(crate) fn reference(&self) -> &str {
        &self.reference
    }
}
