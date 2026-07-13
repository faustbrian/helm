use super::{ProjectRecord, StateStoreError};

/// Durable control-plane state needed independently of any runtime backend.
pub(crate) trait StateStore {
    /// Atomically replaces one project and its complete route ownership set.
    fn replace_project(&mut self, project: &ProjectRecord) -> Result<(), StateStoreError>;

    /// Loads all registered projects in canonical-path order.
    fn projects(&self) -> Result<Vec<ProjectRecord>, StateStoreError>;
}
