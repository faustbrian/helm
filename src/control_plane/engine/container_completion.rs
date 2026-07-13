use super::{EngineFuture, OwnedContainer};
use std::time::Duration;

/// Narrow capability for waiting on an owned one-shot container.
pub(crate) trait ContainerCompletion {
    fn wait_for_success<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        timeout: Duration,
    ) -> EngineFuture<'operation, ()>;
}
