use super::{ContainerId, ManagedResourceMetadata};

/// Engine container identity coupled to reconstructed Stackctl ownership proof.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OwnedContainer {
    id: ContainerId,
    metadata: ManagedResourceMetadata,
}

impl OwnedContainer {
    pub(super) fn new(id: ContainerId, metadata: ManagedResourceMetadata) -> Self {
        Self { id, metadata }
    }

    pub(crate) const fn id(&self) -> &ContainerId {
        &self.id
    }

    pub(crate) const fn metadata(&self) -> &ManagedResourceMetadata {
        &self.metadata
    }
}
