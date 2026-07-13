use super::ContainerId;
use std::collections::BTreeMap;

/// Backend-independent identity and labels returned by an Engine rescan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ObservedContainer {
    id: ContainerId,
    labels: BTreeMap<String, String>,
}

impl ObservedContainer {
    pub(crate) fn new(id: ContainerId, labels: BTreeMap<String, String>) -> Self {
        Self { id, labels }
    }

    pub(crate) const fn id(&self) -> &ContainerId {
        &self.id
    }

    pub(crate) fn labels(&self) -> &BTreeMap<String, String> {
        &self.labels
    }
}
