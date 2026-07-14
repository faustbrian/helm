use super::{EngineFuture, V7ContainerRetirementTarget};

/// Destructive capability limited to an exact accepted v7 container source.
pub(crate) trait V7ContainerRetirement {
    fn retire_v7_container<'operation>(
        &'operation mut self,
        target: &'operation V7ContainerRetirementTarget,
    ) -> EngineFuture<'operation, ()>;
}
