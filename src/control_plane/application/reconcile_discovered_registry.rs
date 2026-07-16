use super::control_plane::project_records;
use super::{ControlPlane, ControlPlaneError, DesiredRegistry};
use crate::control_plane::state::StateStore;

impl<Store> ControlPlane<Store>
where
    Store: StateStore,
{
    /// Atomically publishes one already validated complete discovered registry.
    pub(crate) fn reconcile_discovered_registry(
        &mut self,
        registry: &DesiredRegistry,
        orphaned_at_unix_seconds: i64,
    ) -> Result<(), ControlPlaneError> {
        let records = project_records(registry);
        self.state_store
            .reconcile_project_registry(&records, orphaned_at_unix_seconds)
            .map_err(ControlPlaneError::from)?;
        for project in registry.projects() {
            self.reactivate_project_if_retained(project.project_directory())?;
        }

        Ok(())
    }
}
