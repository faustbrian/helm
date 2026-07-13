use super::{EngineFuture, PublishedPortBinding};

/// Narrow Engine capability for foreign and managed public TCP bindings.
pub(crate) trait PublishedPortDiscovery {
    fn discover_published_tcp_ports(&self) -> EngineFuture<'_, Vec<PublishedPortBinding>>;
}
