use super::ImageId;
use std::collections::BTreeMap;

/// Backend-independent image identity and labels returned by an Engine rescan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ObservedImage {
    id: ImageId,
    labels: BTreeMap<String, String>,
}

impl ObservedImage {
    pub(crate) fn new(id: ImageId, labels: BTreeMap<String, String>) -> Self {
        Self { id, labels }
    }

    pub(crate) const fn id(&self) -> &ImageId {
        &self.id
    }

    pub(crate) fn labels(&self) -> &BTreeMap<String, String> {
        &self.labels
    }
}
