use super::{ControlPlaneError, DesiredRegistry, ProjectSource, plan_project_registry};
use crate::control_plane::state::{ProjectRecord, StateStore};

/// The v8 application boundary coordinating pure plans and durable state.
pub(crate) struct ControlPlane<Store> {
    state_store: Store,
}

impl<Store> ControlPlane<Store>
where
    Store: StateStore,
{
    /// Creates a control plane around one durable state capability.
    pub(crate) fn new(state_store: Store) -> Self {
        Self { state_store }
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
