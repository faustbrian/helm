use super::{EngineFuture, ObservedContainer};

/// Narrow periodic-rescan capability independent of container lifecycle.
pub(crate) trait ContainerDiscovery {
    fn discover_managed(&self) -> EngineFuture<'_, Vec<ObservedContainer>>;
}
