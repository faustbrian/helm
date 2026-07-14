use super::V7MySqlSourceRetirement;
use crate::control_plane::engine::{V7ContainerRetirement, V7ContainerRetirementTarget};
use crate::control_plane::migration::{
    MigrationFuture, MigrationOperationError, V7LogicalDataMigrationSource,
};

/// Engine-backed confirmation cleanup for an accepted v7 MySQL-family source.
pub(crate) struct EngineV7MySqlSourceRetirement<'operation, E> {
    engine: &'operation E,
}

impl<'operation, E> EngineV7MySqlSourceRetirement<'operation, E> {
    pub(crate) const fn new(engine: &'operation E) -> Self {
        Self { engine }
    }
}

impl<E> V7MySqlSourceRetirement for EngineV7MySqlSourceRetirement<'_, E>
where
    E: V7ContainerRetirement + Send + Sync,
{
    fn retire_source<'operation>(
        &'operation mut self,
        source: &'operation V7LogicalDataMigrationSource,
    ) -> MigrationFuture<'operation, ()> {
        Box::pin(async move {
            let container = source.command_target().map_err(|error| {
                MigrationOperationError::new(format!(
                    "v7 MySQL-family retirement target is invalid: {error}"
                ))
            })?;
            let target =
                V7ContainerRetirementTarget::new(container, source.named_volumes().to_vec())
                    .map_err(|error| {
                        MigrationOperationError::new(format!(
                            "v7 MySQL-family retirement volumes are invalid: {error}"
                        ))
                    })?;
            self.engine
                .retire_v7_container(&target)
                .await
                .map_err(|error| {
                    MigrationOperationError::new(format!(
                        "retire accepted v7 MySQL-family source: {error}"
                    ))
                })
        })
    }
}
