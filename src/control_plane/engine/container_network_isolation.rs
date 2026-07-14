use super::{EngineFuture, OwnedContainer, OwnedNetwork};

/// Temporarily removes one exact owned container from its owned network.
pub(crate) trait ContainerNetworkIsolation {
    fn disconnect_container_network<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        network: &'operation OwnedNetwork,
    ) -> EngineFuture<'operation, ()>;

    fn reconnect_container_network<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        network: &'operation OwnedNetwork,
        alias: &'operation str,
    ) -> EngineFuture<'operation, ()>;
}
