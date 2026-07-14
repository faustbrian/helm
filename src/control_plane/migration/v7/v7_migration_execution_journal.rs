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
}
