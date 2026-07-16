use super::{ApplicationContainerRequestOptions, ApplicationHealthCheck};
use crate::control_plane::engine::{
    BindMount, ContainerCreateOptions, ContainerHealthCheck, ContainerRestartPolicy, EngineError,
    LinuxCapability, ResourceKind, RetentionClass,
};
use std::time::Duration;

const PROJECT_SOURCE_TARGET: &str = "/workspace";

/// Produces the exact Engine request for one dedicated project application.
pub(crate) fn application_container_request(
    options: ApplicationContainerRequestOptions,
) -> Result<ContainerCreateOptions, EngineError> {
    if options.environment.project_id() != options.plan.project_id() {
        return Err(EngineError::InvalidRequest {
            detail: format!(
                "application '{}' cannot use runtime environment owned by '{}'",
                options.plan.project_id(),
                options.environment.project_id()
            ),
        });
    }
    if options.metadata.kind() != ResourceKind::ProjectApplication
        || options.metadata.project_id() != Some(options.plan.project_id())
        || options.metadata.retention() != RetentionClass::Disposable
    {
        return Err(EngineError::InvalidRequest {
            detail: format!(
                "application '{}' requires matching disposable project-application ownership metadata",
                options.plan.project_id()
            ),
        });
    }
    let source =
        options
            .plan
            .source_path()
            .to_str()
            .ok_or_else(|| EngineError::InvalidRequest {
                detail: format!(
                    "application source path '{}' is not valid UTF-8",
                    options.plan.source_path().display()
                ),
            })?;

    let request = ContainerCreateOptions::new(
        options.plan.container_name(),
        options.plan.image_digest(),
        options.metadata,
    )?
    .with_platform(options.platform)?
    .with_user(options.container_user)?
    .with_network(options.plan.network_name())?
    .with_bind_mount(BindMount::read_write(source, PROJECT_SOURCE_TARGET)?)
    .with_working_directory(PROJECT_SOURCE_TARGET)?
    .with_environment(options.environment.values().clone())?;
    let request = if options.command.is_empty() {
        request
    } else {
        request.with_command(options.command)?
    };

    let health_command = match options.plan.health_check() {
        ApplicationHealthCheck::Laravel => vec![
            "php".to_owned(),
            "artisan".to_owned(),
            "about".to_owned(),
            "--only=environment".to_owned(),
            "--no-ansi".to_owned(),
        ],
        ApplicationHealthCheck::Tcp => vec![
            "php".to_owned(),
            "-r".to_owned(),
            format!(
                "$socket = @fsockopen('127.0.0.1', {}); exit($socket === false ? 1 : 0);",
                options.plan.internal_http_port()
            ),
        ],
    };
    let application_health_check = ContainerHealthCheck::new(
        health_command,
        Duration::from_secs(10),
        Duration::from_secs(3),
        Duration::from_secs(15),
        5,
    )?;

    Ok(request
        .without_linux_capabilities()
        .with_linux_capability(LinuxCapability::NetBindService)
        .with_health_check(application_health_check)
        .with_restart_policy(ContainerRestartPolicy::UnlessStopped))
}
