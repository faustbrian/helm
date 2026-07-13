use super::{EngineFuture, ObservedVolume};

/// Narrow periodic-rescan capability for Stackctl-marked volumes.
pub(crate) trait VolumeDiscovery {
    fn discover_managed_volumes(&self) -> EngineFuture<'_, Vec<ObservedVolume>>;
}
