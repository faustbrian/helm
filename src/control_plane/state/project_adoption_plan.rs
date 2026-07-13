use super::{ResourceLifecycle, ResourceRecord, StateStoreError};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Complete exact-match state required to reactivate one orphaned project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectAdoptionPlan {
    canonical_path: PathBuf,
    project_id: String,
    resources: Vec<ResourceRecord>,
    credential_ids: Vec<String>,
    environment_revision: String,
}

impl ProjectAdoptionPlan {
    pub(crate) fn new(
        canonical_path: PathBuf,
        project_id: String,
        mut resources: Vec<ResourceRecord>,
        mut credential_ids: Vec<String>,
        environment_revision: String,
    ) -> Result<Self, StateStoreError> {
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
            resources,
            credential_ids,
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
