use serde::{Deserialize, Serialize};

use super::{IpcResourceHealth, IpcResourceLifecycle};

/// One secret-free durable resource projection returned to IPC clients.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcResourceStatus {
    service: String,
    kind: String,
    lifecycle: IpcResourceLifecycle,
    health: IpcResourceHealth,
    observed_at_unix_seconds: Option<i64>,
    shared: bool,
}

impl IpcResourceStatus {
    pub(crate) const fn new(
        service: String,
        kind: String,
        lifecycle: IpcResourceLifecycle,
        health: IpcResourceHealth,
        observed_at_unix_seconds: Option<i64>,
        shared: bool,
    ) -> Self {
        Self {
            service,
            kind,
            lifecycle,
            health,
            observed_at_unix_seconds,
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

    pub(crate) const fn health(&self) -> IpcResourceHealth {
        self.health
    }

    pub(crate) const fn observed_at_unix_seconds(&self) -> Option<i64> {
        self.observed_at_unix_seconds
    }

    pub(crate) const fn shared(&self) -> bool {
        self.shared
    }
}
