use super::{
    WorkloadReconcileAction, WorkloadReconcileError, WorkloadReconcileOptions,
    WorkloadReconcileResult,
};
use crate::control_plane::engine::{
    ContainerDiscovery, ContainerHealth, ContainerLifecycle, ContainerState, EngineError,
    HealthObserver, ObservedResourceOwnership, OwnedContainer, ResourceKind, RetentionClass,
    reconstruct_owned_container,
};

/// Restores one disposable project application without touching other workloads.
pub(crate) async fn reconcile_project_application<E>(
    engine: &mut E,
    options: WorkloadReconcileOptions<'_>,
) -> Result<WorkloadReconcileResult, WorkloadReconcileError>
where
    E: ContainerDiscovery + ContainerLifecycle + HealthObserver,
{
    let project_id = validate_request(&options)?;
    let observed = engine
        .discover_managed()
        .await
        .map_err(|error| engine_error("discover managed containers", error))?;
    let mut applications = Vec::new();

    for container in &observed {
        match reconstruct_owned_container(
            container,
            options.installation_id,
            options.schema_version,
        ) {
            Ok(owned)
                if owned.metadata().kind() == ResourceKind::ProjectApplication
                    && owned.metadata().project_id() == Some(project_id) =>
            {
                applications.push(owned);
            }
            Ok(_) | Err(ObservedResourceOwnership::Unmanaged) => {}
            Err(ObservedResourceOwnership::ForeignInstallation { .. }) => {}
            Err(ownership) => {
                return Err(WorkloadReconcileError::Conflict {
                    detail: format!(
                        "cannot reconcile project '{project_id}' because managed container '{}' has invalid ownership: {ownership:?}",
                        container.id().as_str()
                    ),
                });
            }
        }
    }

    match applications.as_slice() {
        [] => create_application(engine, &options, WorkloadReconcileAction::Created).await,
        [application] => reconcile_existing(engine, &options, application).await,
        applications => Err(WorkloadReconcileError::Conflict {
            detail: format!(
                "project '{project_id}' owns {} application containers; refusing to guess",
                applications.len()
            ),
        }),
    }
}

async fn reconcile_existing<E>(
    engine: &mut E,
    options: &WorkloadReconcileOptions<'_>,
    application: &OwnedContainer,
) -> Result<WorkloadReconcileResult, WorkloadReconcileError>
where
    E: ContainerLifecycle + HealthObserver,
{
    if application.metadata() != options.request.metadata() {
        return replace_application(engine, options, application).await;
    }

    match engine
        .inspect(application)
        .await
        .map_err(|error| engine_error("inspect project application", error))?
    {
        ContainerState::Running => match engine
            .observe_health(application)
            .await
            .map_err(|error| engine_error("observe project application health", error))?
        {
            ContainerHealth::Unhealthy { .. } => {
                engine
                    .stop(application)
                    .await
                    .map_err(|error| engine_error("stop unhealthy project application", error))?;
                engine.start(application).await.map_err(|error| {
                    engine_error("restart unhealthy project application", error)
                })?;
                observe(engine, application, WorkloadReconcileAction::Restarted).await
            }
            health => Ok(WorkloadReconcileResult::new(
                application.clone(),
                WorkloadReconcileAction::Unchanged,
                health,
            )),
        },
        ContainerState::Stopped => {
            engine
                .start(application)
                .await
                .map_err(|error| engine_error("start project application", error))?;
            observe(engine, application, WorkloadReconcileAction::Started).await
        }
        ContainerState::Missing => {
            create_application(engine, options, WorkloadReconcileAction::Created).await
        }
    }
}

async fn replace_application<E>(
    engine: &mut E,
    options: &WorkloadReconcileOptions<'_>,
    application: &OwnedContainer,
) -> Result<WorkloadReconcileResult, WorkloadReconcileError>
where
    E: ContainerLifecycle + HealthObserver,
{
    match engine
        .inspect(application)
        .await
        .map_err(|error| engine_error("inspect drifted project application", error))?
    {
        ContainerState::Running => engine
            .stop(application)
            .await
            .map_err(|error| engine_error("stop drifted project application", error))?,
        ContainerState::Stopped => {}
        ContainerState::Missing => {
            return create_application(engine, options, WorkloadReconcileAction::Created).await;
        }
    }

    engine
        .remove(application)
        .await
        .map_err(|error| engine_error("remove drifted project application", error))?;
    create_application(engine, options, WorkloadReconcileAction::Replaced).await
}

async fn create_application<E>(
    engine: &mut E,
    options: &WorkloadReconcileOptions<'_>,
    action: WorkloadReconcileAction,
) -> Result<WorkloadReconcileResult, WorkloadReconcileError>
where
    E: ContainerLifecycle + HealthObserver,
{
    let application = engine
        .create(options.request)
        .await
        .map_err(|error| engine_error("create project application", error))?;
    engine
        .start(&application)
        .await
        .map_err(|error| engine_error("start created project application", error))?;

    observe(engine, &application, action).await
}

async fn observe<E>(
    engine: &E,
    application: &OwnedContainer,
    action: WorkloadReconcileAction,
) -> Result<WorkloadReconcileResult, WorkloadReconcileError>
where
    E: HealthObserver,
{
    let health = engine
        .observe_health(application)
        .await
        .map_err(|error| engine_error("observe project application health", error))?;

    Ok(WorkloadReconcileResult::new(
        application.clone(),
        action,
        health,
    ))
}

fn validate_request<'request>(
    options: &'request WorkloadReconcileOptions<'_>,
) -> Result<&'request str, WorkloadReconcileError> {
    let metadata = options.request.metadata();
    let project_id =
        metadata
            .project_id()
            .ok_or_else(|| WorkloadReconcileError::InvalidRequest {
                detail: "project application reconciliation requires a project owner".to_owned(),
            })?;
    if metadata.kind() != ResourceKind::ProjectApplication
        || metadata.installation_id() != options.installation_id
        || metadata.schema_version() != options.schema_version
        || metadata.retention() != RetentionClass::Disposable
    {
        return Err(WorkloadReconcileError::InvalidRequest {
            detail: "project application ownership does not match the active installation"
                .to_owned(),
        });
    }

    Ok(project_id)
}

fn engine_error(action: &str, error: EngineError) -> WorkloadReconcileError {
    WorkloadReconcileError::Engine {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}
