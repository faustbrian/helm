use super::{EngineFuture, NetworkCreateOptions, OwnedNetwork};

/// Narrow replaceable strategy for Stackctl-owned Engine networks.
pub(crate) trait NetworkManager {
    fn create_network<'operation>(
        &'operation mut self,
        options: &'operation NetworkCreateOptions,
    ) -> EngineFuture<'operation, OwnedNetwork>;

    fn remove_network<'operation>(
        &'operation mut self,
        network: &'operation OwnedNetwork,
    ) -> EngineFuture<'operation, ()>;
}
