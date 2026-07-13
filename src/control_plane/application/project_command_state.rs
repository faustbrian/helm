use super::{ControlPlane, ControlPlaneError};
use crate::control_plane::state::{ManagedEnvironmentRecord, ProjectRecord, StateStore};

impl<Store> ControlPlane<Store>
where
    Store: StateStore,
{
    /// Loads exact registered project paths for command target validation.
    pub(crate) fn projects(&self) -> Result<Vec<ProjectRecord>, ControlPlaneError> {
        self.state_store.projects().map_err(Into::into)
    }

    /// Loads daemon-owned values injected into in-container commands.
    pub(crate) fn managed_environments(
        &self,
    ) -> Result<Vec<ManagedEnvironmentRecord>, ControlPlaneError> {
        self.state_store.managed_environments().map_err(Into::into)
    }
}
