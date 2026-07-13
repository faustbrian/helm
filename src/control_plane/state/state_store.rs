use super::{ProjectRecord, ResourceRecord, StateStoreError};

/// Durable control-plane state needed independently of any runtime backend.
pub(crate) trait StateStore {
    /// Atomically replaces one project and its complete route ownership set.
    fn replace_project(&mut self, project: &ProjectRecord) -> Result<(), StateStoreError> {
        self.replace_projects(std::slice::from_ref(project))
    }

    /// Atomically replaces a complete validated batch of discovered projects.
    fn replace_projects(&mut self, projects: &[ProjectRecord]) -> Result<(), StateStoreError>;

    /// Loads all registered projects in canonical-path order.
    fn projects(&self) -> Result<Vec<ProjectRecord>, StateStoreError>;

    /// Atomically unregisters a project and orphans its project-owned resources.
    fn orphan_project(
        &mut self,
        canonical_path: &std::path::Path,
        orphaned_at_unix_seconds: i64,
    ) -> Result<(), StateStoreError>;

    /// Upserts observed ownership without implicitly deleting missing resources.
    fn upsert_resources(&mut self, resources: &[ResourceRecord]) -> Result<(), StateStoreError>;

    /// Loads all durable resources in stable backend-identity order.
    fn resources(&self) -> Result<Vec<ResourceRecord>, StateStoreError>;
}
