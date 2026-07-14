use crate::control_plane::migration::{MigrationCutoverPlan, MigrationRollbackPlan};
use crate::control_plane::state::{StateStore, StateStoreError, V7MigrationExecutionRecord};
use std::path::Path;

/// Minimal durable boundary required by the v7 migration coordinator.
pub(crate) trait V7MigrationExecutionJournal {
    fn load_v7_execution(
        &self,
        canonical_project_path: &Path,
        evidence_revision: &str,
    ) -> Result<Option<V7MigrationExecutionRecord>, StateStoreError>;

    fn persist_v7_execution(
        &mut self,
        execution: &V7MigrationExecutionRecord,
    ) -> Result<(), StateStoreError>;

    fn persist_v7_cutover(
        &mut self,
        desired_state: &MigrationCutoverPlan,
        execution: &V7MigrationExecutionRecord,
    ) -> Result<(), StateStoreError>;

    fn persist_v7_rollback(
        &mut self,
        restored_state: &MigrationRollbackPlan,
        execution: &V7MigrationExecutionRecord,
    ) -> Result<(), StateStoreError>;
}

impl<Store> V7MigrationExecutionJournal for Store
where
    Store: StateStore + ?Sized,
{
    fn load_v7_execution(
        &self,
        canonical_project_path: &Path,
        evidence_revision: &str,
    ) -> Result<Option<V7MigrationExecutionRecord>, StateStoreError> {
        StateStore::v7_migration_execution(self, canonical_project_path, evidence_revision)
    }

    fn persist_v7_execution(
        &mut self,
        execution: &V7MigrationExecutionRecord,
    ) -> Result<(), StateStoreError> {
        StateStore::record_v7_migration_execution(self, execution)
    }

    fn persist_v7_cutover(
        &mut self,
        desired_state: &MigrationCutoverPlan,
        execution: &V7MigrationExecutionRecord,
    ) -> Result<(), StateStoreError> {
        StateStore::record_v7_migration_cutover(
            self,
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
        StateStore::record_v7_migration_rollback(
            self,
            restored_state.project(),
            restored_state.environment(),
            restored_state.retained_targets(),
            execution,
        )
    }
}
