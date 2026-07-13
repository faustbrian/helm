use super::{
    LogicalResourceRecord, ProjectAdoptionPlanOptions, ResourceLifecycle, ResourceRecord,
    StateStoreError,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Complete exact-match state required to reactivate one orphaned project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectAdoptionPlan {
    canonical_path: PathBuf,
    project_id: String,
    resources: Vec<ResourceRecord>,
    logical_resources: Vec<LogicalResourceRecord>,
    credential_ids: Vec<String>,
    environment_revision: String,
}

impl ProjectAdoptionPlan {
    pub(crate) fn new(mut options: ProjectAdoptionPlanOptions) -> Result<Self, StateStoreError> {
        let ProjectAdoptionPlanOptions {
            canonical_path,
            project_id,
            ref mut resources,
            ref mut logical_resources,
            ref mut credential_ids,
            environment_revision,
        } = options;
        if canonical_path.as_os_str().is_empty()
            || project_id.is_empty()
            || environment_revision.is_empty()
        {
            return Err(invalid(
                "project path, identity, and environment revision are required",
            ));
        }
        if resources.iter().any(|resource| {
            resource.project_id() != Some(project_id.as_str())
                || resource.lifecycle() != ResourceLifecycle::Active
                || resource.orphaned_at_unix_seconds().is_some()
        }) {
            return Err(invalid(
                "every adopted resource must be active, non-orphaned, and owned by the project",
            ));
        }
        resources.sort_by(|first, second| first.resource_id().cmp(second.resource_id()));
        let resource_count = resources
            .iter()
            .map(ResourceRecord::resource_id)
            .collect::<BTreeSet<_>>()
            .len();
        if resource_count != resources.len() {
            return Err(invalid("adopted resource identities must be unique"));
        }
        if logical_resources.iter().any(|resource| {
            resource.project_id() != project_id
                || resource.lifecycle() != ResourceLifecycle::Active
                || resource.orphaned_at_unix_seconds().is_some()
        }) {
            return Err(invalid(
                "every adopted logical resource must be active, non-orphaned, and owned by the project",
            ));
        }
        logical_resources.sort_by(|first, second| {
            first
                .logical_resource_id()
                .cmp(second.logical_resource_id())
        });
        let logical_resource_count = logical_resources
            .iter()
            .map(LogicalResourceRecord::logical_resource_id)
            .collect::<BTreeSet<_>>()
            .len();
        if logical_resource_count != logical_resources.len() {
            return Err(invalid(
                "adopted logical resource identities must be unique",
            ));
        }
        if credential_ids.iter().any(String::is_empty) {
            return Err(invalid("adopted credential identities must not be empty"));
        }
        credential_ids.sort();
        if credential_ids.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(invalid("adopted credential identities must be unique"));
        }

        Ok(Self {
            canonical_path,
            project_id,
            resources: std::mem::take(resources),
            logical_resources: std::mem::take(logical_resources),
            credential_ids: std::mem::take(credential_ids),
            environment_revision,
        })
    }

    pub(crate) fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.project_id
    }

    pub(crate) fn resources(&self) -> &[ResourceRecord] {
        &self.resources
    }

    pub(crate) fn logical_resources(&self) -> &[LogicalResourceRecord] {
        &self.logical_resources
    }

    pub(crate) fn credential_ids(&self) -> &[String] {
        &self.credential_ids
    }

    pub(crate) fn environment_revision(&self) -> &str {
        &self.environment_revision
    }
}

fn invalid(detail: &str) -> StateStoreError {
    StateStoreError::InvalidProjectAdoption {
        detail: detail.to_owned(),
    }
}
