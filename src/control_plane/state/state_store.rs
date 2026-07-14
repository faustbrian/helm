use super::{
    CredentialRecord, DaemonEventRecord, DaemonOperationRecord, DaemonOperationTransitionOptions,
    InstallationLifecycle, InstallationRecord, LogicalResourceRecord, ManagedEnvironmentRecord,
    MigrationRecord, ProjectAdoptionPlan, ProjectRecord, RecoveryPointRecord, ResourceRecord,
    StateStoreError,
};
use std::path::{Path, PathBuf};

/// Durable control-plane state needed independently of any runtime backend.
pub(crate) trait StateStore: Send {
    /// Initializes immutable installation identity, or verifies an exact replay.
    fn initialize_installation(
        &mut self,
        installation: &InstallationRecord,
    ) -> Result<(), StateStoreError>;

    /// Loads the selected installation and Engine endpoint when initialized.
    fn installation(&self) -> Result<Option<InstallationRecord>, StateStoreError>;

    /// Loads whether normal reconciliation is active or deletion has begun.
    fn installation_lifecycle(&self) -> Result<Option<InstallationLifecycle>, StateStoreError>;

    /// Atomically freezes discovery and orphans every registered project.
    fn begin_installation_deletion(
        &mut self,
        orphaned_at_unix_seconds: i64,
    ) -> Result<(), StateStoreError>;

    /// Finalizes teardown only after all logical tenants have been retired.
    fn complete_installation_deletion(&mut self) -> Result<(), StateStoreError>;

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

    /// Replaces the complete valid scan and atomically orphans missing projects.
    fn reconcile_project_registry(
        &mut self,
        projects: &[ProjectRecord],
        orphaned_at_unix_seconds: i64,
    ) -> Result<(), StateStoreError>;

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

    /// Replaces active physical identities within each exact ownership scope.
    fn reconcile_resources(
        &mut self,
        resources: &[ResourceRecord],
        replaced_at_unix_seconds: i64,
    ) -> Result<(), StateStoreError>;

    /// Atomically reactivates exact retained state for one registered project.
    fn adopt_project(&mut self, adoption: &ProjectAdoptionPlan) -> Result<(), StateStoreError>;

    /// Loads all durable resources in stable backend-identity order.
    fn resources(&self) -> Result<Vec<ResourceRecord>, StateStoreError>;

    /// Atomically forgets exact non-active resource snapshots after backend deletion.
    fn retire_resources(&mut self, resources: &[ResourceRecord]) -> Result<(), StateStoreError>;

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

    /// Atomically forgets one exact orphaned tenant and its disabled secret.
    fn retire_logical_resource(
        &mut self,
        resource: &LogicalResourceRecord,
        credential: &CredentialRecord,
    ) -> Result<(), StateStoreError>;

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

    /// Atomically publishes logical tenant ownership and its project environment.
    fn record_logical_environment(
        &mut self,
        resources: &[LogicalResourceRecord],
        environment: &ManagedEnvironmentRecord,
    ) -> Result<(), StateStoreError>;

    /// Replaces one project's active logical set and orphans omitted services.
    fn reconcile_logical_environment(
        &mut self,
        resources: &[LogicalResourceRecord],
        environment: &ManagedEnvironmentRecord,
        orphaned_at_unix_seconds: i64,
    ) -> Result<(), StateStoreError>;

    /// Records one monotonic, crash-recoverable migration checkpoint.
    fn record_migration(&mut self, migration: &MigrationRecord) -> Result<(), StateStoreError>;

    /// Atomically owns a provisioned target and advances its migration journal.
    fn record_migration_target(
        &mut self,
        target: &LogicalResourceRecord,
        credential: &CredentialRecord,
        migration: &MigrationRecord,
    ) -> Result<(), StateStoreError>;

    /// Atomically switches project routes, environment, and migration proof.
    fn record_migration_cutover(
        &mut self,
        project: &ProjectRecord,
        environment: &ManagedEnvironmentRecord,
        migration: &MigrationRecord,
    ) -> Result<(), StateStoreError>;

    /// Atomically restores project state, retains targets, and journals rollback.
    fn record_migration_rollback(
        &mut self,
        project: &ProjectRecord,
        environment: &ManagedEnvironmentRecord,
        retained_targets: &[LogicalResourceRecord],
        migration: &MigrationRecord,
    ) -> Result<(), StateStoreError>;

    /// Loads migration checkpoints in stable identity order.
    fn migrations(&self) -> Result<Vec<MigrationRecord>, StateStoreError>;

    /// Inserts immutable verified recovery evidence, allowing exact replay only.
    fn record_recovery_point(
        &mut self,
        recovery_point: &RecoveryPointRecord,
    ) -> Result<(), StateStoreError>;

    /// Loads one project's recovery points newest-first.
    fn recovery_points(
        &self,
        project_id: &str,
    ) -> Result<Vec<RecoveryPointRecord>, StateStoreError>;

    /// Atomically appends one event and retains only the newest bounded set.
    fn append_daemon_event(
        &mut self,
        operation_id: &str,
        kind_json: &str,
        retention_limit: usize,
    ) -> Result<DaemonEventRecord, StateStoreError>;

    /// Loads retained daemon events in monotonic sequence order.
    fn daemon_events(&self) -> Result<Vec<DaemonEventRecord>, StateStoreError>;

    /// Atomically persists a queued operation and its accepted event.
    fn enqueue_daemon_operation(
        &mut self,
        operation: &DaemonOperationRecord,
        accepted_kind_json: &str,
        event_retention_limit: usize,
    ) -> Result<DaemonEventRecord, StateStoreError>;

    /// Advances an exact operation lifecycle and optionally appends one event.
    fn transition_daemon_operation(
        &mut self,
        options: DaemonOperationTransitionOptions<'_>,
    ) -> Result<Option<DaemonEventRecord>, StateStoreError>;

    /// Loads one exact operation, including terminal history, for idempotency.
    fn daemon_operation(
        &self,
        operation_id: &str,
    ) -> Result<Option<DaemonOperationRecord>, StateStoreError>;

    /// Loads non-terminal operations in creation order for restart recovery.
    fn active_daemon_operations(&self) -> Result<Vec<DaemonOperationRecord>, StateStoreError>;
}
