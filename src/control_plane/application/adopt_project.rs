use super::{ControlPlane, ControlPlaneError};
use crate::control_plane::state::{
    CredentialLifecycle, LogicalResourceRecord, LogicalResourceRecordOptions, ProjectAdoptionPlan,
    ProjectAdoptionPlanOptions, ResourceLifecycle, ResourceRecord, ResourceRecordOptions,
    StateStore, StateStoreError,
};
use std::path::Path;

impl<Store> ControlPlane<Store>
where
    Store: StateStore,
{
    /// Atomically reactivates the complete retained inventory for one exact path.
    pub(crate) fn adopt_project(
        &mut self,
        canonical_path: &Path,
    ) -> Result<String, ControlPlaneError> {
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
        let project_id = project.project_name().to_owned();
        let resources = self
            .state_store
            .resources()?
            .into_iter()
            .filter(|resource| {
                resource.project_id() == Some(project_id.as_str())
                    && resource.lifecycle() == ResourceLifecycle::Orphaned
            })
            .map(|resource| {
                let mut active = ResourceRecord::new(ResourceRecordOptions {
                    resource_id: resource.resource_id().to_owned(),
                    installation_id: resource.installation_id().to_owned(),
                    kind: resource.kind().to_owned(),
                    compatibility_fingerprint: resource.compatibility_fingerprint().to_owned(),
                    project_id: Some(project_id.clone()),
                    schema_version: resource.schema_version(),
                    desired_revision: resource.desired_revision().to_owned(),
                    retention: resource.retention(),
                    lifecycle: ResourceLifecycle::Active,
                    orphaned_at_unix_seconds: None,
                });
                if let Some(scope_id) = resource.scope_id() {
                    active = active.with_scope_id(scope_id);
                }

                active
            })
            .collect();
        let logical_resources = self
            .state_store
            .logical_resources()?
            .into_iter()
            .filter(|resource| {
                resource.project_id() == project_id
                    && resource.lifecycle() == ResourceLifecycle::Orphaned
            })
            .map(|resource| {
                LogicalResourceRecord::new(LogicalResourceRecordOptions {
                    logical_resource_id: resource.logical_resource_id().to_owned(),
                    shared_resource_id: resource.shared_resource_id().to_owned(),
                    project_id: project_id.clone(),
                    service_id: resource.service_id().to_owned(),
                    kind: resource.kind().to_owned(),
                    compatibility_fingerprint: resource.compatibility_fingerprint().to_owned(),
                    desired_revision: resource.desired_revision().to_owned(),
                    lifecycle: ResourceLifecycle::Active,
                    orphaned_at_unix_seconds: None,
                })
            })
            .collect();
        let credential_ids = self
            .state_store
            .credentials()?
            .into_iter()
            .filter(|credential| {
                credential.project_id() == Some(project_id.as_str())
                    && credential.lifecycle() == CredentialLifecycle::Disabled
            })
            .map(|credential| credential.credential_id().to_owned())
            .collect();
        let environment = self
            .state_store
            .managed_environments()?
            .into_iter()
            .find(|environment| environment.project_id() == project_id)
            .ok_or_else(|| StateStoreError::ProjectAdoptionStateMismatch {
                project_id: project_id.clone(),
                detail: "retained managed environment is missing".to_owned(),
            })?;
        let adoption = ProjectAdoptionPlan::new(ProjectAdoptionPlanOptions {
            canonical_path: canonical_path.to_path_buf(),
            project_id: project_id.clone(),
            resources,
            logical_resources,
            credential_ids,
            environment_revision: environment.revision().to_owned(),
        })?;

        self.state_store.adopt_project(&adoption)?;

        Ok(project_id)
    }
}
