use super::ManagedResourceMetadata;

/// Engine volume identity coupled to reconstructed Stackctl ownership proof.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OwnedVolume {
    name: String,
    metadata: ManagedResourceMetadata,
}

impl OwnedVolume {
    pub(super) fn new(name: impl Into<String>, metadata: ManagedResourceMetadata) -> Self {
        Self {
            name: name.into(),
            metadata,
        }
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) const fn metadata(&self) -> &ManagedResourceMetadata {
        &self.metadata
    }
}
