use super::ImageId;
use std::collections::BTreeMap;

/// Backend-independent image identity and labels returned by an Engine rescan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ObservedImage {
    id: ImageId,
    created_at_unix_seconds: i64,
    container_count: i64,
    labels: BTreeMap<String, String>,
}

impl ObservedImage {
    pub(crate) fn new(
        id: ImageId,
        created_at_unix_seconds: i64,
        container_count: i64,
        labels: BTreeMap<String, String>,
    ) -> Self {
        Self {
            id,
            created_at_unix_seconds,
            container_count,
            labels,
        }
    }

    pub(crate) const fn id(&self) -> &ImageId {
        &self.id
    }

    pub(crate) const fn created_at_unix_seconds(&self) -> i64 {
        self.created_at_unix_seconds
    }

    pub(crate) const fn container_count(&self) -> i64 {
        self.container_count
    }

    pub(crate) fn labels(&self) -> &BTreeMap<String, String> {
        &self.labels
    }
}
