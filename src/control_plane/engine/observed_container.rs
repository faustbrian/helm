use super::{ContainerId, ObservedContainerMount};
use std::collections::BTreeMap;

/// Backend-independent identity and labels returned by an Engine rescan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ObservedContainer {
    id: ContainerId,
    labels: BTreeMap<String, String>,
    image_identity: Option<String>,
    mounts: Vec<ObservedContainerMount>,
}

impl ObservedContainer {
    pub(crate) fn new(id: ContainerId, labels: BTreeMap<String, String>) -> Self {
        Self {
            id,
            labels,
            image_identity: None,
            mounts: Vec::new(),
        }
    }

    pub(crate) const fn id(&self) -> &ContainerId {
        &self.id
    }

    pub(crate) fn labels(&self) -> &BTreeMap<String, String> {
        &self.labels
    }

    pub(crate) fn with_image_identity(mut self, image_identity: impl Into<String>) -> Self {
        let image_identity = image_identity.into();
        self.image_identity = (!image_identity.is_empty()).then_some(image_identity);
        self
    }

    pub(crate) fn image_identity(&self) -> Option<&str> {
        self.image_identity.as_deref()
    }

    pub(crate) fn with_mounts(mut self, mounts: Vec<ObservedContainerMount>) -> Self {
        self.mounts = mounts;
        self
    }

    pub(crate) fn mounts(&self) -> &[ObservedContainerMount] {
        &self.mounts
    }
}
