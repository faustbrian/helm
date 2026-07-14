use super::{GatewayError, GatewaySnapshot};
use std::future::Future;
use std::pin::Pin;

/// A nonblocking operation against a replaceable gateway provider.
pub(crate) type GatewayFuture<'operation, Output> =
    Pin<Box<dyn Future<Output = Result<Output, GatewayError>> + Send + 'operation>>;

/// Applies complete serialized route state, never incremental per-project edits.
pub(crate) trait GatewayConfiguration {
    fn apply_snapshot<'operation>(
        &'operation mut self,
        snapshot: &'operation GatewaySnapshot,
    ) -> GatewayFuture<'operation, ()>;

    fn active_revision(&self) -> GatewayFuture<'_, Option<String>>;
}
