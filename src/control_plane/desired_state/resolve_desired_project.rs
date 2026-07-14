use super::{DesiredProject, DesiredProjectError, DesiredService, DesiredServiceOptions};
use crate::control_plane::configuration::RawProjectConfig;
use crate::control_plane::{
    ProjectIdentity, RouteClaim, ServiceIdentity, is_valid_environment_variable_key,
    resolve_service_deployment_strategy,
};
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
    let mut routable_services = Vec::new();

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

        let preset = optional_non_empty(name, "preset", raw_service.preset())?;
        let deployment_strategy = preset
            .as_deref()
            .map(resolve_service_deployment_strategy)
            .transpose()
            .map_err(|error| invalid_service(name, error.to_string()))?;
        let image = optional_non_empty(name, "image", raw_service.image())?;
        if preset.is_none() && image.is_none() {
            return Err(invalid_service(
                name,
                "must declare at least a preset or image",
            ));
        }
        if deployment_strategy.is_some_and(|strategy| strategy.claims_gateway_route())
            || (deployment_strategy.is_none() && image.is_some())
        {
            routable_services.push(identity.clone());
        }
        let version = optional_non_empty(name, "version", raw_service.version())?;
        let database = optional_non_empty(name, "database", raw_service.database())?;
        let command = validate_command(name, raw_service.command())?;
        let environment = validate_environment(name, raw_service.environment())?;
        let mut php_extensions = raw_service.php_extensions().to_vec();
        php_extensions.sort();
        for pair in php_extensions.windows(2) {
            if pair[0] == pair[1] {
                return Err(invalid_service(
                    name,
                    format!("declares PHP extension '{}' more than once", pair[0]),
                ));
            }
        }
        for extension in &php_extensions {
            if !valid_php_extension(extension) {
                return Err(invalid_service(
                    name,
                    format!("declares invalid PHP extension '{extension}'"),
                ));
            }
        }
        if !php_extensions.is_empty() && !preset.as_deref().is_some_and(extension_capable_preset) {
            return Err(invalid_service(
                name,
                format!(
                    "declares PHP extensions but preset '{}' does not provide the pinned \
                     install-php-extensions runtime contract",
                    preset.as_deref().unwrap_or("<none>")
                ),
            ));
        }

        services.insert(
            name.clone(),
            DesiredService::new(DesiredServiceOptions {
                identity,
                dependencies,
                preset,
                image,
                version,
                php_extensions,
                database,
                command,
                environment,
            }),
        );
    }

    validate_dependencies_exist(&services)?;
    let startup_order = resolve_startup_order(&services)?;
    let route_claims = routable_services
        .into_iter()
        .map(|service| RouteClaim::new(project_directory.to_path_buf(), project.clone(), service))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(DesiredProject::new(
        project,
        project_directory.to_path_buf(),
        services,
        startup_order,
        route_claims,
    ))
}

fn extension_capable_preset(preset: &str) -> bool {
    matches!(preset, "frankenphp" | "laravel" | "reverb")
}

fn validate_command(
    service: &str,
    command: Option<&[String]>,
) -> Result<Option<Vec<String>>, DesiredProjectError> {
    let Some(command) = command else {
        return Ok(None);
    };

    if command.first().is_none_or(String::is_empty) {
        return Err(invalid_service(
            service,
            "command must contain a non-empty executable",
        ));
    }
    if command.iter().any(|argument| argument.contains('\0')) {
        return Err(invalid_service(
            service,
            "command must not contain NUL bytes",
        ));
    }

    Ok(Some(command.to_vec()))
}

fn validate_environment(
    service: &str,
    environment: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, DesiredProjectError> {
    for (key, value) in environment {
        if !is_valid_environment_variable_key(key) {
            return Err(invalid_service(
                service,
                format!("declares invalid environment key '{key}'"),
            ));
        }
        if value.contains('\0') {
            return Err(invalid_service(
                service,
                format!("environment value for '{key}' must not contain NUL bytes"),
            ));
        }
    }

    Ok(environment.clone())
}

fn optional_non_empty(
    service: &str,
    field: &str,
    value: Option<&str>,
) -> Result<Option<String>, DesiredProjectError> {
    match value {
        Some("") => Err(invalid_service(service, format!("has an empty {field}"))),
        Some(value) if value.trim() != value => {
            Err(invalid_service(service, format!("has an invalid {field}")))
        }
        Some(value) => Ok(Some(value.to_owned())),
        None => Ok(None),
    }
}

fn valid_php_extension(extension: &str) -> bool {
    !extension.is_empty()
        && extension.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

fn invalid_service(service: &str, detail: impl Into<String>) -> DesiredProjectError {
    DesiredProjectError::InvalidService {
        service: service.to_owned(),
        detail: detail.into(),
    }
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
