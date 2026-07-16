use super::{ControlPlane, ControlPlaneError};
use crate::control_plane::state::{
    CredentialLifecycle, EnvironmentLifecycle, ResourceLifecycle, StateStore, StateStoreError,
};
use std::path::Path;

impl<Store> ControlPlane<Store>
where
    Store: StateStore,
{
    /// Reactivates exact retained ownership for a currently registered path.
    pub(super) fn reactivate_project_if_retained(
        &mut self,
        canonical_path: &Path,
    ) -> Result<bool, ControlPlaneError> {
        let project = self
            .state_store
            .projects()?
            .into_iter()
            .find(|project| project.canonical_path() == canonical_path)
            .ok_or_else(|| StateStoreError::InvalidProjectAdoption {
                detail: format!(
                    "project path '{}' is not registered",
                    canonical_path.display()
                ),
            })?;
        let project_id = project.project_name();
        let has_retained_state = self.state_store.resources()?.iter().any(|resource| {
            resource.project_id() == Some(project_id)
                && resource.lifecycle() == ResourceLifecycle::Orphaned
        }) || self.state_store.logical_resources()?.iter().any(
            |resource| {
                resource.project_id() == project_id
                    && resource.lifecycle() == ResourceLifecycle::Orphaned
            },
        ) || self.state_store.credentials()?.iter().any(|credential| {
            credential.project_id() == Some(project_id)
                && credential.lifecycle() == CredentialLifecycle::Disabled
        }) || self.state_store.managed_environments()?.iter().any(
            |environment| {
                environment.project_id() == project_id
                    && environment.lifecycle() == EnvironmentLifecycle::Disabled
            },
        );

        if !has_retained_state {
            return Ok(false);
        }

        self.adopt_project(canonical_path)?;

        Ok(true)
    }
}
