use super::{ImageId, ManagedResourceMetadata};

/// Engine image identity coupled to reconstructed Stackctl ownership proof.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OwnedImage {
    id: ImageId,
    metadata: ManagedResourceMetadata,
}

impl OwnedImage {
    pub(super) const fn new(id: ImageId, metadata: ManagedResourceMetadata) -> Self {
        Self { id, metadata }
    }

    pub(crate) const fn id(&self) -> &ImageId {
        &self.id
    }

    pub(crate) const fn metadata(&self) -> &ManagedResourceMetadata {
        &self.metadata
    }
}
