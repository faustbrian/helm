use crate::control_plane::engine::EngineError;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

/// One bounded asynchronous direct-Engine connection attempt.
pub(crate) type EngineConnectionFuture<'operation, Engine> =
    Pin<Box<dyn Future<Output = Result<Engine, EngineError>> + Send + 'operation>>;

/// Replaceable connector for one installation-selected Engine endpoint.
pub(crate) trait EngineConnector {
    type Engine;

    fn connect<'operation>(
        &'operation mut self,
        endpoint: &'operation Path,
    ) -> EngineConnectionFuture<'operation, Self::Engine>;
}
