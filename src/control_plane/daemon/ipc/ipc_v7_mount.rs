use serde::{Deserialize, Serialize};

/// Secret-free configured or Engine-observed legacy mount.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcV7Mount {
    source_kind: String,
    source: String,
    target: String,
    read_only: bool,
}

impl IpcV7Mount {
    pub(crate) const fn new(
        source_kind: String,
        source: String,
        target: String,
        read_only: bool,
    ) -> Self {
        Self {
            source_kind,
            source,
            target,
            read_only,
        }
    }

    pub(crate) fn source_kind(&self) -> &str {
        &self.source_kind
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn target(&self) -> &str {
        &self.target
    }

    pub(crate) const fn is_read_only(&self) -> bool {
        self.read_only
    }
}
