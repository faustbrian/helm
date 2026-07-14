use super::{ControlPlane, ControlPlaneError};
use crate::control_plane::state::{StateStore, V7MigrationExecutionRecord};
use std::path::Path;

impl<Store> ControlPlane<Store>
where
    Store: StateStore,
{
    /// Persists one monotonic project-wide v7 migration execution.
    pub(crate) fn record_v7_migration_execution(
        &mut self,
        execution: &V7MigrationExecutionRecord,
    ) -> Result<(), ControlPlaneError> {
        self.state_store
            .record_v7_migration_execution(execution)
            .map_err(Into::into)
    }

    /// Loads the execution bound to one exact accepted source revision.
    pub(crate) fn v7_migration_execution(
        &self,
        canonical_project_path: &Path,
        evidence_revision: &str,
    ) -> Result<Option<V7MigrationExecutionRecord>, ControlPlaneError> {
        self.state_store
            .v7_migration_execution(canonical_project_path, evidence_revision)
            .map_err(Into::into)
    }

    /// Loads every durable project-wide v7 execution.
    pub(crate) fn v7_migration_executions(
        &self,
    ) -> Result<Vec<V7MigrationExecutionRecord>, ControlPlaneError> {
        self.state_store
            .v7_migration_executions()
            .map_err(Into::into)
    }
}
