use super::RegistryPlanError;
use crate::control_plane::DesiredProject;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// Rejects identity aliasing before credentials or logical resources exist.
pub(super) fn validate_project_identities(
    projects: &[DesiredProject],
) -> Result<(), RegistryPlanError> {
    let mut paths_by_project = BTreeMap::<String, BTreeSet<PathBuf>>::new();

    for project in projects {
        paths_by_project
            .entry(project.project_name().to_owned())
            .or_default()
            .insert(project.project_directory().to_path_buf());
    }

    let conflicts = paths_by_project
        .into_iter()
        .filter(|(_, paths)| paths.len() > 1)
        .map(|(project, paths)| (project, paths.into_iter().collect()))
        .collect::<Vec<_>>();

    if conflicts.is_empty() {
        Ok(())
    } else {
        Err(RegistryPlanError::ProjectIdentityOwnership { conflicts })
    }
}
