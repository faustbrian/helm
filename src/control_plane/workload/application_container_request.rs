use super::ApplicationContainerRequestOptions;
use crate::control_plane::engine::{
    BindMount, ContainerCreateOptions, ContainerRestartPolicy, EngineError, ResourceKind,
    RetentionClass,
};

const PROJECT_SOURCE_TARGET: &str = "/workspace";

/// Produces the exact Engine request for one dedicated project application.
pub(crate) fn application_container_request(
    options: ApplicationContainerRequestOptions,
) -> Result<ContainerCreateOptions, EngineError> {
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

    ContainerCreateOptions::new(
        options.plan.container_name(),
        options.plan.image_digest(),
        options.metadata,
    )?
    .with_platform(options.platform)?
    .with_network(options.plan.network_name())?
    .with_bind_mount(BindMount::read_write(source, PROJECT_SOURCE_TARGET)?)
    .with_command(options.command)?
    .with_environment(options.environment)
    .map(|request| request.with_restart_policy(ContainerRestartPolicy::UnlessStopped))
}
