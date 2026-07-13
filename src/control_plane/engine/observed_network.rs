use super::NetworkId;
use std::collections::BTreeMap;

/// Backend-independent network identity and labels returned by an Engine rescan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ObservedNetwork {
    id: NetworkId,
    labels: BTreeMap<String, String>,
}

impl ObservedNetwork {
    pub(crate) fn new(id: NetworkId, labels: BTreeMap<String, String>) -> Self {
        Self { id, labels }
    }

    pub(crate) const fn id(&self) -> &NetworkId {
        &self.id
    }

    pub(crate) fn labels(&self) -> &BTreeMap<String, String> {
        &self.labels
    }
}
