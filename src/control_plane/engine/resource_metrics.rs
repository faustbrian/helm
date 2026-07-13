use super::{ContainerResourceMetrics, EngineFuture, OwnedContainer};

/// Narrow Engine capability for ownership-scoped resource measurements.
pub(crate) trait ResourceMetrics {
    fn sample_resources<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ContainerResourceMetrics>;
}
