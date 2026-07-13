use super::{ContainerLogOptions, EngineError, EngineFuture, LogChunk, OwnedContainer};
use futures_util::Stream;
use std::pin::Pin;

/// Object-safe asynchronous stream of raw container log frames.
pub(crate) type ContainerLogStream<'stream> =
    Pin<Box<dyn Stream<Item = Result<LogChunk, EngineError>> + Send + 'stream>>;

/// Narrow Engine capability for ownership-scoped streaming logs.
pub(crate) trait LogSource {
    fn logs<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        options: &'operation ContainerLogOptions,
    ) -> EngineFuture<'operation, ContainerLogStream<'operation>>;
}
