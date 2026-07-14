use super::{EngineFuture, ObservedImage};

/// Narrow periodic-rescan capability for Stackctl-marked images.
pub(crate) trait ImageDiscovery {
    fn discover_managed_images(&self) -> EngineFuture<'_, Vec<ObservedImage>>;
}
