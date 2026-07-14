use super::{EngineFuture, ObservedContainer};

/// Read-only discovery of v7 marker-owned containers during explicit migration.
pub(crate) trait LegacyContainerDiscovery {
    fn discover_v7_managed(&self) -> EngineFuture<'_, Vec<ObservedContainer>>;
}
