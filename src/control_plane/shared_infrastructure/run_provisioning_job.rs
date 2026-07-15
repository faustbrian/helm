use super::{ProvisioningJobOptions, SharedInfrastructureReconcileError};
use crate::control_plane::engine::{
    ContainerCompletion, ContainerDiscovery, ContainerLifecycle, ContainerState, EngineError,
    ImageResolver, ImmutableImageReference, ObservedResourceOwnership, OwnedContainer,
    ResourceKind, RetentionClass, reconstruct_owned_container,
};

const KIND_LABEL: &str = "dev.stackctl.kind";
const PROJECT_LABEL: &str = "dev.stackctl.project";
const RESOURCE_LABEL: &str = "dev.stackctl.resource";
const PROVISIONING_JOB_KIND: &str = "provisioning_job";

/// Runs one deterministic private-network job and removes it after completion.
pub(crate) async fn run_provisioning_job<E>(
    engine: &mut E,
    options: ProvisioningJobOptions<'_>,
) -> Result<(), SharedInfrastructureReconcileError>
where
    E: ContainerCompletion + ContainerDiscovery + ContainerLifecycle + ImageResolver,
{
    validate_request(&options)?;
    let metadata = options.request.metadata();
    let project_id = metadata.project_id().unwrap_or_default();
    let resource_id = metadata.resource_id().unwrap_or_default();
    let observed = engine
        .discover_managed()
        .await
        .map_err(|error| engine_error("provisioning job discovery", error))?;
    let mut matching = Vec::new();

    for container in observed.iter().filter(|container| {
        container.labels().get(KIND_LABEL).map(String::as_str) == Some(PROVISIONING_JOB_KIND)
            && container.labels().get(PROJECT_LABEL).map(String::as_str) == Some(project_id)
            && container.labels().get(RESOURCE_LABEL).map(String::as_str) == Some(resource_id)
    }) {
        match reconstruct_owned_container(
            container,
            options.installation_id,
            options.schema_version,
        ) {
            Ok(owned) => matching.push(owned),
            Err(ObservedResourceOwnership::ForeignInstallation { .. }) => {
                return Err(SharedInfrastructureReconcileError::Conflict {
                    detail: format!(
                        "provisioning job '{project_id}/{resource_id}' is owned by another installation"
                    ),
                });
            }
            Err(ownership) => {
                return Err(SharedInfrastructureReconcileError::Conflict {
                    detail: format!(
                        "provisioning job '{project_id}/{resource_id}' has invalid ownership: {ownership:?}"
                    ),
                });
            }
        }
    }

    match matching.as_slice() {
        [] => create_and_run(engine, &options).await,
        [container] => match engine
            .inspect(container)
            .await
            .map_err(|error| engine_error("provisioning job inspection", error))?
        {
            ContainerState::Missing => create_and_run(engine, &options).await,
            ContainerState::Running | ContainerState::Stopped
                if container.metadata() == options.request.metadata() =>
            {
                complete_and_remove(engine, container, options.timeout).await
            }
            ContainerState::Running | ContainerState::Stopped => {
                complete_and_remove(engine, container, options.timeout).await?;
                create_and_run(engine, &options).await
            }
        },
        jobs => Err(SharedInfrastructureReconcileError::Conflict {
            detail: format!(
                "provisioning job '{project_id}/{resource_id}' owns {} containers; refusing to guess",
                jobs.len()
            ),
        }),
    }
}

async fn create_and_run<E>(
    engine: &mut E,
    options: &ProvisioningJobOptions<'_>,
) -> Result<(), SharedInfrastructureReconcileError>
where
    E: ContainerCompletion + ContainerLifecycle + ImageResolver,
{
    let image = ImmutableImageReference::new(options.request.image())
        .map_err(|error| engine_error("validate provisioning image", error))?;
    engine
        .ensure_image(&image)
        .await
        .map_err(|error| engine_error("ensure provisioning image", error))?;
    let container = engine
        .create(options.request)
        .await
        .map_err(|error| engine_error("provisioning job creation", error))?;
    if container.metadata() != options.request.metadata() {
        return Err(SharedInfrastructureReconcileError::Engine {
            action: "provisioning job creation".to_owned(),
            detail: "Engine returned a provisioning job with unexpected ownership".to_owned(),
        });
    }
    engine
        .start(&container)
        .await
        .map_err(|error| engine_error("provisioning job start", error))?;
    complete_and_remove(engine, &container, options.timeout).await
}

async fn complete_and_remove<E>(
    engine: &mut E,
    container: &OwnedContainer,
    timeout: std::time::Duration,
) -> Result<(), SharedInfrastructureReconcileError>
where
    E: ContainerCompletion + ContainerLifecycle,
{
    let completion = match engine.wait_for_success(container, timeout).await {
        Err(error @ EngineError::Timeout { .. }) => {
            return Err(engine_error("provisioning job completion", error));
        }
        result => result,
    };
    let removal = engine.remove(container).await;

    match (completion, removal) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(completion @ EngineError::ContainerExit { .. }), Ok(())) => {
            Err(SharedInfrastructureReconcileError::ProvisioningFailed {
                detail: completion.to_string(),
            })
        }
        (Err(completion), Ok(())) => Err(engine_error("provisioning job completion", completion)),
        (Ok(()), Err(removal)) => Err(engine_error("provisioning job removal", removal)),
        (Err(completion), Err(removal)) => Err(SharedInfrastructureReconcileError::Engine {
            action: "provisioning job completion and removal".to_owned(),
            detail: format!("{completion}; cleanup also failed: {removal}"),
        }),
    }
}

fn validate_request(
    options: &ProvisioningJobOptions<'_>,
) -> Result<(), SharedInfrastructureReconcileError> {
    let metadata = options.request.metadata();
    if metadata.kind() != ResourceKind::ProvisioningJob
        || metadata.project_id().is_none()
        || metadata.resource_id().is_none()
        || metadata.installation_id() != options.installation_id
        || metadata.schema_version() != options.schema_version
        || metadata.retention() != RetentionClass::Disposable
        || options.timeout.is_zero()
    {
        return Err(SharedInfrastructureReconcileError::InvalidRequest {
            detail: "provisioning job ownership and timeout must be complete and disposable"
                .to_owned(),
        });
    }

    Ok(())
}

fn engine_error(action: &str, error: EngineError) -> SharedInfrastructureReconcileError {
    SharedInfrastructureReconcileError::Engine {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}
