use super::{DesiredRegistry, ProjectSource, RegistryPlanError};
use crate::control_plane::configuration::parse_project_config;
use crate::control_plane::{resolve_desired_project, validate_route_claims};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Parses and validates the complete deduplicated discovered project set.
pub(crate) fn plan_project_registry(
    sources: &[ProjectSource],
) -> Result<DesiredRegistry, RegistryPlanError> {
    let mut sources_by_path = BTreeMap::<PathBuf, &ProjectSource>::new();

    for source in sources {
        sources_by_path
            .entry(source.canonical_path().to_path_buf())
            .or_insert(source);
    }

    let mut projects = Vec::with_capacity(sources_by_path.len());

    for source in sources_by_path.into_values() {
        let raw = parse_project_config(source.yaml(), source.config_path())?;
        projects.push(resolve_desired_project(raw, source.canonical_path())?);
    }

    let claims = projects
        .iter()
        .flat_map(|project| project.route_claims().iter().cloned())
        .collect();
    validate_route_claims(claims)?;

    Ok(DesiredRegistry::new(projects))
}
