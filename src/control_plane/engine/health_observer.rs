use super::{ContainerHealth, EngineFuture, OwnedContainer};

/// Narrow Engine capability for ownership-scoped process and health state.
pub(crate) trait HealthObserver {
    fn observe_health<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ContainerHealth>;
}
