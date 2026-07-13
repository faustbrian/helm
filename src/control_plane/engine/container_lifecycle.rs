use super::{ContainerCreateOptions, ContainerState, EngineError, OwnedContainer};
use std::future::Future;
use std::pin::Pin;

/// An object-safe nonblocking Engine operation.
pub(crate) type EngineFuture<'operation, Output> =
    Pin<Box<dyn Future<Output = Result<Output, EngineError>> + Send + 'operation>>;

/// Narrow container lifecycle effects implemented by Docker and Podman adapters.
pub(crate) trait ContainerLifecycle {
    fn create<'operation>(
        &'operation mut self,
        options: &'operation ContainerCreateOptions,
    ) -> EngineFuture<'operation, OwnedContainer>;

    fn start<'operation>(
        &'operation mut self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()>;

    fn stop<'operation>(
        &'operation mut self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()>;

    fn remove<'operation>(
        &'operation mut self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()>;

    fn inspect<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ContainerState>;
}
