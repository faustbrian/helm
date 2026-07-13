use super::{EngineFuture, NetworkCreateOptions, NetworkId};

/// Narrow replaceable strategy for Stackctl-owned Engine networks.
pub(crate) trait NetworkManager {
    fn create_network<'operation>(
        &'operation mut self,
        options: &'operation NetworkCreateOptions,
    ) -> EngineFuture<'operation, NetworkId>;

    fn remove_network<'operation>(
        &'operation mut self,
        network: &'operation NetworkId,
    ) -> EngineFuture<'operation, ()>;
}
