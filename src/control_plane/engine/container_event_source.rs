use super::{ContainerEvent, ContainerEventCursor, EngineError};
use futures_util::Stream;
use std::pin::Pin;

/// Object-safe asynchronous stream of structured Engine events.
pub(crate) type ContainerEventStream<'stream> =
    Pin<Box<dyn Stream<Item = Result<ContainerEvent, EngineError>> + Send + 'stream>>;

/// Narrow Engine capability for prompt managed-container reconciliation.
pub(crate) trait ContainerEventSource {
    fn stream_managed<'stream>(
        &'stream self,
        installation_id: &'stream str,
        cursor: ContainerEventCursor,
    ) -> ContainerEventStream<'stream>;
}
