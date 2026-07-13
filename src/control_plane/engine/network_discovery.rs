use super::{EngineFuture, ObservedNetwork};

/// Narrow periodic-rescan capability for Stackctl-marked networks.
pub(crate) trait NetworkDiscovery {
    fn discover_managed_networks(&self) -> EngineFuture<'_, Vec<ObservedNetwork>>;
}
