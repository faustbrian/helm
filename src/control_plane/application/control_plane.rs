use super::{ControlPlaneError, DesiredRegistry, ProjectSource, plan_project_registry};
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, PreparedSharedInstance, SharedInstancePlan, SharedPreparationError,
    SharedPreparationOptions, prepare_shared_instances,
};
use crate::control_plane::state::{ProjectRecord, ResourceRecord, StateStore};
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

    pub(crate) fn record_resources(
        &mut self,
        resources: &[ResourceRecord],
        replaced_at_unix_seconds: i64,
    ) -> Result<(), ControlPlaneError> {
        self.state_store
            .reconcile_resources(resources, replaced_at_unix_seconds)
            .map_err(Into::into)
    }

    /// Plans all sources, then atomically persists the validated batch.
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
