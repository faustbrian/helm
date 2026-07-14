use super::{ControlPlane, ControlPlaneError};
use crate::control_plane::migration::{
    MigrationCutoverPlan, MigrationRollbackPlan, V7MigrationExecutionJournal,
};
use crate::control_plane::state::{StateStore, StateStoreError, V7MigrationExecutionRecord};
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

impl<Store> V7MigrationExecutionJournal for ControlPlane<Store>
where
    Store: StateStore,
{
    fn load_v7_execution(
        &self,
        canonical_project_path: &Path,
        evidence_revision: &str,
    ) -> Result<Option<V7MigrationExecutionRecord>, StateStoreError> {
        self.state_store
            .v7_migration_execution(canonical_project_path, evidence_revision)
    }

    fn persist_v7_execution(
        &mut self,
        execution: &V7MigrationExecutionRecord,
    ) -> Result<(), StateStoreError> {
        self.state_store.record_v7_migration_execution(execution)
    }

    fn persist_v7_cutover(
        &mut self,
        desired_state: &MigrationCutoverPlan,
        execution: &V7MigrationExecutionRecord,
    ) -> Result<(), StateStoreError> {
        self.state_store.record_v7_migration_cutover(
            desired_state.project(),
            desired_state.environment(),
            execution,
        )
    }

    fn persist_v7_rollback(
        &mut self,
        restored_state: &MigrationRollbackPlan,
        execution: &V7MigrationExecutionRecord,
    ) -> Result<(), StateStoreError> {
        self.state_store.record_v7_migration_rollback(
            restored_state.project(),
            restored_state.environment(),
            restored_state.retained_targets(),
            execution,
        )
    }
}
