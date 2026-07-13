use std::collections::BTreeMap;

/// Backend-independent volume name and labels returned by an Engine rescan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ObservedVolume {
    name: String,
    labels: BTreeMap<String, String>,
}

impl ObservedVolume {
    pub(crate) fn new(name: impl Into<String>, labels: BTreeMap<String, String>) -> Self {
        Self {
            name: name.into(),
            labels,
        }
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn labels(&self) -> &BTreeMap<String, String> {
        &self.labels
    }
}
