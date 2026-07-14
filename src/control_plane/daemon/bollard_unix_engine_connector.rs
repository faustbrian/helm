use super::{EngineConnectionFuture, EngineConnector};
use crate::control_plane::engine::{BollardEngineAdapter, bounded_engine_operation};
use std::path::Path;
use std::time::Duration;

/// Production connector for the persisted Docker-compatible Unix socket.
pub(crate) struct BollardUnixEngineConnector {
    timeout: Duration,
}

impl BollardUnixEngineConnector {
    pub(crate) const fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl EngineConnector for BollardUnixEngineConnector {
    type Engine = BollardEngineAdapter;

    fn connect<'operation>(
        &'operation mut self,
        endpoint: &'operation Path,
    ) -> EngineConnectionFuture<'operation, Self::Engine> {
        Box::pin(async move {
            bounded_engine_operation(
                "connect to selected Engine endpoint",
                self.timeout,
                BollardEngineAdapter::connect_unix(endpoint),
            )
            .await
        })
    }
}
