use super::{ControlPlaneError, DesiredRegistry, ProjectSource, plan_project_registry};
use crate::control_plane::ExecutionPlan;
use crate::control_plane::project_infrastructure::{
    PreparedProjectService, ProjectServicePreparationError, prepare_project_services,
};
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, PreparedSharedInstance, SharedInstancePlan, SharedPreparationError,
    SharedPreparationOptions, prepare_shared_instances,
};
use crate::control_plane::state::{
    InstallationLifecycle, ProjectRecord, ResourceRecord, StateStore,
};
use std::path::PathBuf;

/// The v8 application boundary coordinating pure plans and durable state.
pub(crate) struct ControlPlane<Store> {
    pub(super) state_store: Store,
}

impl<Store> ControlPlane<Store>
where
    Store: StateStore,
{
    /// Creates a control plane around one durable state capability.
    pub(crate) fn new(state_store: Store) -> Self {
        Self { state_store }
    }

    /// Loads the complete authoritative watched-root set.
    pub(crate) fn watched_roots(&self) -> Result<Vec<PathBuf>, ControlPlaneError> {
        self.state_store.watched_roots().map_err(Into::into)
    }

    /// Loads the durable installation lifecycle gate.
    pub(crate) fn installation_lifecycle(
        &self,
    ) -> Result<Option<InstallationLifecycle>, ControlPlaneError> {
        self.state_store
            .installation_lifecycle()
            .map_err(Into::into)
    }

    /// Loads durable physical ownership for lifecycle reconciliation.
    pub(crate) fn resources(&self) -> Result<Vec<ResourceRecord>, ControlPlaneError> {
        self.state_store.resources().map_err(Into::into)
    }

    pub(crate) fn prepare_shared(
        &mut self,
        shared: &[SharedInstancePlan],
        entropy: &impl CredentialEntropy,
        options: SharedPreparationOptions<'_>,
    ) -> Result<Vec<PreparedSharedInstance>, SharedPreparationError> {
        prepare_shared_instances(&mut self.state_store, shared, entropy, options)
    }

    pub(crate) fn prepare_project_services(
        &mut self,
        execution: &ExecutionPlan,
        entropy: &impl CredentialEntropy,
    ) -> Result<Vec<PreparedProjectService>, ProjectServicePreparationError> {
        prepare_project_services(&mut self.state_store, execution, entropy)
    }

    pub(crate) fn record_resources(
        &mut self,
        resources: &[ResourceRecord],
        replaced_at_unix_seconds: i64,
    ) -> Result<(), ControlPlaneError> {
        self.state_store
            .reconcile_resources(resources, replaced_at_unix_seconds)
            .map_err(Into::into)
    }

    /// Retires exact non-active records after their backend objects are absent.
    pub(crate) fn retire_resources(
        &mut self,
        resources: &[ResourceRecord],
    ) -> Result<(), ControlPlaneError> {
        self.state_store
            .retire_resources(resources)
            .map_err(Into::into)
    }

    /// Plans all sources, then atomically persists the validated batch.
    #[cfg(test)]
    pub(crate) fn reconcile_projects(
        &mut self,
        sources: &[ProjectSource],
    ) -> Result<DesiredRegistry, ControlPlaneError> {
        let registry = plan_project_registry(sources)?;
        let records = project_records(&registry);
        self.state_store.replace_projects(&records)?;

        Ok(registry)
    }

    /// Validates a complete scan before atomically applying adds and removals.
    pub(crate) fn reconcile_discovered_projects(
        &mut self,
        sources: &[ProjectSource],
        orphaned_at_unix_seconds: i64,
    ) -> Result<DesiredRegistry, ControlPlaneError> {
        let registry = plan_project_registry(sources)?;
        let records = project_records(&registry);
        self.state_store
            .reconcile_project_registry(&records, orphaned_at_unix_seconds)?;

        Ok(registry)
    }
}

fn project_records(registry: &DesiredRegistry) -> Vec<ProjectRecord> {
    registry
        .projects()
        .iter()
        .map(|project| {
            ProjectRecord::new(
                project.project_directory().to_path_buf(),
                project.project_name().to_owned(),
                project
                    .route_domains()
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            )
        })
        .collect()
}
