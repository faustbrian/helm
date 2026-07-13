use super::{ManagedResourceMetadata, NetworkId};

/// Engine network identity coupled to reconstructed Stackctl ownership proof.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OwnedNetwork {
    id: NetworkId,
    metadata: ManagedResourceMetadata,
}

impl OwnedNetwork {
    pub(super) fn new(id: NetworkId, metadata: ManagedResourceMetadata) -> Self {
        Self { id, metadata }
    }

    pub(crate) const fn id(&self) -> &NetworkId {
        &self.id
    }

    pub(crate) const fn metadata(&self) -> &ManagedResourceMetadata {
        &self.metadata
    }
}
