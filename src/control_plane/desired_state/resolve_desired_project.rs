use super::{DesiredProject, DesiredProjectError, DesiredService};
use crate::control_plane::configuration::RawProjectConfig;
use crate::control_plane::{ProjectIdentity, RouteClaim, ServiceIdentity};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VisitState {
    Visiting,
    Visited,
}

/// Resolves raw configuration into pure, dependency-ordered desired state.
pub(crate) fn resolve_desired_project(
    raw: RawProjectConfig,
    project_directory: &Path,
) -> Result<DesiredProject, DesiredProjectError> {
    let project = ProjectIdentity::resolve(raw.project(), project_directory)?;
    let mut services = BTreeMap::new();

    for (name, raw_service) in raw.services() {
        let identity = ServiceIdentity::new(name)?;
        let dependency_names = raw_service
            .depends_on()
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let dependencies = dependency_names
            .iter()
            .map(|dependency| ServiceIdentity::new(dependency))
            .collect::<Result<Vec<_>, _>>()?;

        services.insert(name.clone(), DesiredService::new(identity, dependencies));
    }

    validate_dependencies_exist(&services)?;
    let startup_order = resolve_startup_order(&services)?;
    let route_claims = services
        .values()
        .map(|service| {
            RouteClaim::new(
                project_directory.to_path_buf(),
                project.clone(),
                service.identity().clone(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(DesiredProject::new(
        project,
        project_directory.to_path_buf(),
        services,
        startup_order,
        route_claims,
    ))
}

fn validate_dependencies_exist(
    services: &BTreeMap<String, DesiredService>,
) -> Result<(), DesiredProjectError> {
    for (service_name, service) in services {
        for dependency in service.dependencies() {
            if !services.contains_key(dependency.as_str()) {
                return Err(DesiredProjectError::UnknownDependency {
                    service: service_name.clone(),
                    dependency: dependency.as_str().to_owned(),
                });
            }
        }
    }

    Ok(())
}

fn resolve_startup_order(
    services: &BTreeMap<String, DesiredService>,
) -> Result<Vec<String>, DesiredProjectError> {
    let mut states = BTreeMap::new();
    let mut stack = Vec::new();
    let mut ordered = Vec::new();

    for service_name in services.keys() {
        visit_service(
            service_name,
            services,
            &mut states,
            &mut stack,
            &mut ordered,
        )?;
    }

    Ok(ordered)
}

fn visit_service(
    service_name: &str,
    services: &BTreeMap<String, DesiredService>,
    states: &mut BTreeMap<String, VisitState>,
    stack: &mut Vec<String>,
    ordered: &mut Vec<String>,
) -> Result<(), DesiredProjectError> {
    match states.get(service_name) {
        Some(VisitState::Visited) => return Ok(()),
        Some(VisitState::Visiting) => {
            let cycle = stack
                .iter()
                .skip_while(|name| name.as_str() != service_name)
                .cloned()
                .chain(std::iter::once(service_name.to_owned()))
                .collect();

            return Err(DesiredProjectError::DependencyCycle { cycle });
        }
        None => {}
    }

    states.insert(service_name.to_owned(), VisitState::Visiting);
    stack.push(service_name.to_owned());

    if let Some(service) = services.get(service_name) {
        for dependency in service.dependencies() {
            visit_service(dependency.as_str(), services, states, stack, ordered)?;
        }
    }

    stack.pop();
    states.insert(service_name.to_owned(), VisitState::Visited);
    ordered.push(service_name.to_owned());

    Ok(())
}
