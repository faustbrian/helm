use super::ProjectProcessRequestOptions;
use crate::control_plane::engine::{
    BindMount, ContainerCreateOptions, ContainerRestartPolicy, EngineError, ResourceKind,
    RetentionClass,
};

const PROJECT_SOURCE_TARGET: &str = "/workspace";

/// Produces one Engine request for a supervised long-lived worker.
pub(crate) fn project_process_request(
    options: ProjectProcessRequestOptions,
) -> Result<ContainerCreateOptions, EngineError> {
    if options.metadata.kind() != ResourceKind::ProjectProcess
        || options.metadata.project_id() != Some(options.plan.project_id())
        || options.metadata.resource_id() != Some(options.plan.service_id())
        || options.metadata.retention() != RetentionClass::Disposable
    {
        return Err(EngineError::InvalidRequest {
            detail: format!(
                "project process '{}:{}' requires matching disposable project-process ownership metadata",
                options.plan.project_id(),
                options.plan.service_id()
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
                    "project process source path '{}' is not valid UTF-8",
                    options.plan.source_path().display()
                ),
            })?;

    ContainerCreateOptions::new(
        options.plan.container_name(),
        options.plan.image_digest(),
        options.metadata,
    )?
    .with_platform(options.platform)?
    .with_user(options.container_user)?
    .with_network(options.plan.network_name())?
    .with_bind_mount(BindMount::read_write(source, PROJECT_SOURCE_TARGET)?)
    .with_working_directory(PROJECT_SOURCE_TARGET)?
    .with_command(options.plan.command().to_vec())?
    .with_environment(options.plan.environment().values().clone())
    .map(|request| {
        request
            .without_image_health_check()
            .with_restart_policy(ContainerRestartPolicy::UnlessStopped)
    })
}
