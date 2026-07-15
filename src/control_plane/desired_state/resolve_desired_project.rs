use super::{
    DesiredProject, DesiredProjectError, DesiredService, DesiredServiceOptions,
    SUPPORTED_PHP_EXTENSIONS, resolve_runtime_image_reference::resolve_runtime_image_reference,
    supports_php_extension,
};
use crate::control_plane::configuration::{RawProjectConfig, RawWorkflowConfig};
use crate::control_plane::{
    ProjectIdentity, RouteClaim, ServiceDeploymentStrategy, ServiceIdentity,
    is_valid_environment_variable_key, resolve_service_deployment_strategy,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

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
        let environment_mapping =
            validate_environment_mapping(name, raw_service.environment_mapping())?;
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
            if !supports_php_extension(extension) {
                return Err(invalid_service(
                    name,
                    format!(
                        "declares unsupported PHP extension '{extension}'; supported extensions: {}",
                        SUPPORTED_PHP_EXTENSIONS.join(", ")
                    ),
                ));
            }
        }
        if !php_extensions.is_empty() && !preset.as_deref().is_some_and(extension_capable_preset) {
            return Err(invalid_service(
                name,
                format!(
                    "declares PHP extensions but preset '{}' does not provide the pinned \
                     Stackctl-owned PHP extension runtime contract",
                    preset.as_deref().unwrap_or("<none>")
                ),
            ));
        }
        let composer_image =
            resolve_runtime_image_reference(name, "composer_image", raw_service.composer_image())?;
        let node_image =
            resolve_runtime_image_reference(name, "node_image", raw_service.node_image())?;
        let bun_image =
            resolve_runtime_image_reference(name, "bun_image", raw_service.bun_image())?;
        let is_project_application = deployment_strategy
            == Some(ServiceDeploymentStrategy::ProjectApplication)
            || (deployment_strategy.is_none() && image.is_some());
        if (composer_image.is_some() || node_image.is_some() || bun_image.is_some())
            && !is_project_application
        {
            return Err(invalid_service(
                name,
                "runtime tool images are supported only by project application services",
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
                composer_image,
                node_image,
                bun_image,
                database,
                command,
                environment,
                environment_mapping,
            }),
        );
    }

    validate_workflows(raw.workflows(), &services, &routable_services)?;
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

fn validate_workflows(
    workflows: &BTreeMap<String, RawWorkflowConfig>,
    services: &BTreeMap<String, DesiredService>,
    routable_services: &[ServiceIdentity],
) -> Result<(), DesiredProjectError> {
    for (name, workflow) in workflows {
        ServiceIdentity::new(name)?;
        if workflow.steps().is_empty() {
            return Err(invalid_service(
                &format!("workflow {name}"),
                "must declare at least one step",
            ));
        }
        for step in workflow.steps() {
            let service = services.get(step.service()).ok_or_else(|| {
                invalid_service(
                    &format!("workflow {name}"),
                    format!("references unknown service '{}'", step.service()),
                )
            })?;
            if let Some(file) = step.file() {
                if !matches!(service.preset(), Some("mysql" | "mariadb")) {
                    return Err(invalid_service(
                        &format!("workflow {name}"),
                        format!(
                            "database restore service '{}' must use MySQL or MariaDB",
                            step.service()
                        ),
                    ));
                }
                validate_workflow_path(name, "database dump", file)?;
                if let Some(entry) = step.archive_entry() {
                    validate_workflow_path(name, "database dump archive entry", Path::new(entry))?;
                    if file.extension().and_then(|value| value.to_str()) != Some("zip") {
                        return Err(invalid_service(
                            &format!("workflow {name}"),
                            "archive_entry requires a .zip database dump",
                        ));
                    }
                }
                if let Some(migration_service) = step.migration_service() {
                    let migration = services.get(migration_service).ok_or_else(|| {
                        invalid_service(
                            &format!("workflow {name}"),
                            format!("references unknown migration service '{migration_service}'"),
                        )
                    })?;
                    if migration.preset().is_some_and(|preset| {
                        resolve_service_deployment_strategy(preset).ok()
                            != Some(ServiceDeploymentStrategy::ProjectApplication)
                    }) {
                        return Err(invalid_service(
                            &format!("workflow {name}"),
                            format!(
                                "migration service '{migration_service}' is not an application"
                            ),
                        ));
                    }
                    let connection = step.migration_connection().unwrap_or_default();
                    if connection.is_empty()
                        || !connection
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
                    {
                        return Err(invalid_service(
                            &format!("workflow {name}"),
                            "declares an invalid Laravel migration connection",
                        ));
                    }
                }
            } else if !routable_services
                .iter()
                .any(|identity| identity.as_str() == step.service())
            {
                return Err(invalid_service(
                    &format!("workflow {name}"),
                    format!("open service '{}' has no gateway route", step.service()),
                ));
            }
        }
    }

    Ok(())
}

fn validate_workflow_path(
    workflow: &str,
    label: &str,
    path: &Path,
) -> Result<(), DesiredProjectError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(invalid_service(
            &format!("workflow {workflow}"),
            format!("{label} must be a relative project file without traversal"),
        ));
    }

    Ok(())
}

fn validate_environment_mapping(
    service: &str,
    mapping: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, DesiredProjectError> {
    let mut targets = BTreeSet::new();
    for (source, target) in mapping {
        if !is_valid_environment_variable_key(source) {
            return Err(invalid_service(
                service,
                format!("declares invalid generated environment key '{source}'"),
            ));
        }
        if !is_valid_environment_variable_key(target) {
            return Err(invalid_service(
                service,
                format!("declares invalid mapped environment key '{target}'"),
            ));
        }
        if !targets.insert(target) {
            return Err(invalid_service(
                service,
                format!("maps multiple generated values to environment key '{target}'"),
            ));
        }
    }

    Ok(mapping.clone())
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

pub(super) fn invalid_service(service: &str, detail: impl Into<String>) -> DesiredProjectError {
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
