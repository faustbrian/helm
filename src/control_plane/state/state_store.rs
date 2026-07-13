use super::{
    CredentialRecord, InstallationRecord, LogicalResourceRecord, ManagedEnvironmentRecord,
    ProjectAdoptionPlan, ProjectRecord, ResourceRecord, StateStoreError,
};
use std::path::{Path, PathBuf};

/// Durable control-plane state needed independently of any runtime backend.
pub(crate) trait StateStore {
    /// Initializes immutable installation identity, or verifies an exact replay.
    fn initialize_installation(
        &mut self,
        installation: &InstallationRecord,
    ) -> Result<(), StateStoreError>;

    /// Loads the selected installation and Engine endpoint when initialized.
    fn installation(&self) -> Result<Option<InstallationRecord>, StateStoreError>;

    /// Atomically replaces the complete set of canonical watched roots.
    fn replace_watched_roots(&mut self, roots: &[PathBuf]) -> Result<(), StateStoreError>;

    /// Loads canonical watched roots in stable path order.
    fn watched_roots(&self) -> Result<Vec<PathBuf>, StateStoreError>;

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
        canonical_path: &Path,
        orphaned_at_unix_seconds: i64,
    ) -> Result<(), StateStoreError>;

    /// Upserts observed ownership without implicitly deleting missing resources.
    fn upsert_resources(&mut self, resources: &[ResourceRecord]) -> Result<(), StateStoreError>;

    /// Atomically reactivates exact retained state for one registered project.
    fn adopt_project(&mut self, adoption: &ProjectAdoptionPlan) -> Result<(), StateStoreError>;

    /// Loads all durable resources in stable backend-identity order.
    fn resources(&self) -> Result<Vec<ResourceRecord>, StateStoreError>;

    /// Upserts logical tenant ownership without deleting missing retained data.
    fn upsert_logical_resources(
        &mut self,
        resources: &[LogicalResourceRecord],
    ) -> Result<(), StateStoreError>;

    /// Loads all logical tenant resources in stable identity order.
    fn logical_resources(&self) -> Result<Vec<LogicalResourceRecord>, StateStoreError>;

    /// Counts active logical consumers of one shared Engine resource.
    fn active_logical_reference_count(
        &self,
        shared_resource_id: &str,
    ) -> Result<u64, StateStoreError>;

    /// Inserts a credential once, returning the stable existing value on replay.
    fn insert_credential_if_absent(
        &mut self,
        credential: &CredentialRecord,
    ) -> Result<CredentialRecord, StateStoreError>;

    /// Loads all retained credentials in stable identity order.
    fn credentials(&self) -> Result<Vec<CredentialRecord>, StateStoreError>;

    /// Atomically replaces the daemon-owned environment for one project.
    fn replace_managed_environment(
        &mut self,
        environment: &ManagedEnvironmentRecord,
    ) -> Result<(), StateStoreError>;

    /// Loads all retained managed environments in stable project order.
    fn managed_environments(&self) -> Result<Vec<ManagedEnvironmentRecord>, StateStoreError>;
}
