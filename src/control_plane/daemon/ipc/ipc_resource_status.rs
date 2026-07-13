use serde::{Deserialize, Serialize};

use super::IpcResourceLifecycle;

/// One secret-free durable resource projection returned to IPC clients.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcResourceStatus {
    service: String,
    kind: String,
    lifecycle: IpcResourceLifecycle,
    shared: bool,
}

impl IpcResourceStatus {
    pub(crate) const fn new(
        service: String,
        kind: String,
        lifecycle: IpcResourceLifecycle,
        shared: bool,
    ) -> Self {
        Self {
            service,
            kind,
            lifecycle,
            shared,
        }
    }

    pub(crate) fn service(&self) -> &str {
        &self.service
    }

    pub(crate) fn kind(&self) -> &str {
        &self.kind
    }

    pub(crate) const fn lifecycle(&self) -> IpcResourceLifecycle {
        self.lifecycle
    }

    pub(crate) const fn shared(&self) -> bool {
        self.shared
    }
}
