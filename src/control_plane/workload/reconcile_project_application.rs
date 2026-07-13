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
    reconcile_project_workload(engine, options, ResourceKind::ProjectApplication).await
}

pub(super) async fn reconcile_project_workload<E>(
    engine: &mut E,
    options: WorkloadReconcileOptions<'_>,
    expected_kind: ResourceKind,
) -> Result<WorkloadReconcileResult, WorkloadReconcileError>
where
    E: ContainerDiscovery + ContainerLifecycle + HealthObserver,
{
    let (project_id, resource_id) = validate_request(&options, expected_kind)?;
    let observed = engine
        .discover_managed()
        .await
        .map_err(|error| engine_error("discover managed containers", error))?;
    let mut workloads = Vec::new();

    for container in &observed {
        match reconstruct_owned_container(
            container,
            options.installation_id,
            options.schema_version,
        ) {
            Ok(owned)
                if owned.metadata().kind() == expected_kind
                    && owned.metadata().project_id() == Some(project_id.as_str())
                    && owned.metadata().resource_id() == resource_id.as_deref() =>
            {
                workloads.push(owned);
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

    match workloads.as_slice() {
        [] => {
            create_workload(
                engine,
                &options,
                expected_kind,
                WorkloadReconcileAction::Created,
            )
            .await
        }
        [workload] => reconcile_existing(engine, &options, expected_kind, workload).await,
        workloads => Err(WorkloadReconcileError::Conflict {
            detail: duplicate_detail(
                &project_id,
                resource_id.as_deref(),
                expected_kind,
                workloads.len(),
            ),
        }),
    }
}

async fn reconcile_existing<E>(
    engine: &mut E,
    options: &WorkloadReconcileOptions<'_>,
    kind: ResourceKind,
    workload: &OwnedContainer,
) -> Result<WorkloadReconcileResult, WorkloadReconcileError>
where
    E: ContainerLifecycle + HealthObserver,
{
    if workload.metadata() != options.request.metadata() {
        return replace_workload(engine, options, kind, workload).await;
    }

    match engine
        .inspect(workload)
        .await
        .map_err(|error| engine_error(format!("inspect {}", kind_name(kind)), error))?
    {
        ContainerState::Running => {
            match engine.observe_health(workload).await.map_err(|error| {
                engine_error(format!("observe {} health", kind_name(kind)), error)
            })? {
                ContainerHealth::Unhealthy { .. } => {
                    engine.stop(workload).await.map_err(|error| {
                        engine_error(format!("stop unhealthy {}", kind_name(kind)), error)
                    })?;
                    engine.start(workload).await.map_err(|error| {
                        engine_error(format!("restart unhealthy {}", kind_name(kind)), error)
                    })?;
                    observe(engine, workload, kind, WorkloadReconcileAction::Restarted).await
                }
                health => Ok(WorkloadReconcileResult::new(
                    workload.clone(),
                    WorkloadReconcileAction::Unchanged,
                    health,
                )),
            }
        }
        ContainerState::Stopped => {
            engine
                .start(workload)
                .await
                .map_err(|error| engine_error(format!("start {}", kind_name(kind)), error))?;
            observe(engine, workload, kind, WorkloadReconcileAction::Started).await
        }
        ContainerState::Missing => {
            create_workload(engine, options, kind, WorkloadReconcileAction::Created).await
        }
    }
}

async fn replace_workload<E>(
    engine: &mut E,
    options: &WorkloadReconcileOptions<'_>,
    kind: ResourceKind,
    workload: &OwnedContainer,
) -> Result<WorkloadReconcileResult, WorkloadReconcileError>
where
    E: ContainerLifecycle + HealthObserver,
{
    match engine
        .inspect(workload)
        .await
        .map_err(|error| engine_error(format!("inspect drifted {}", kind_name(kind)), error))?
    {
        ContainerState::Running => engine
            .stop(workload)
            .await
            .map_err(|error| engine_error(format!("stop drifted {}", kind_name(kind)), error))?,
        ContainerState::Stopped => {}
        ContainerState::Missing => {
            return create_workload(engine, options, kind, WorkloadReconcileAction::Created).await;
        }
    }

    engine
        .remove(workload)
        .await
        .map_err(|error| engine_error(format!("remove drifted {}", kind_name(kind)), error))?;
    create_workload(engine, options, kind, WorkloadReconcileAction::Replaced).await
}

async fn create_workload<E>(
    engine: &mut E,
    options: &WorkloadReconcileOptions<'_>,
    kind: ResourceKind,
    action: WorkloadReconcileAction,
) -> Result<WorkloadReconcileResult, WorkloadReconcileError>
where
    E: ContainerLifecycle + HealthObserver,
{
    let workload = engine
        .create(options.request)
        .await
        .map_err(|error| engine_error(format!("create {}", kind_name(kind)), error))?;
    engine
        .start(&workload)
        .await
        .map_err(|error| engine_error(format!("start created {}", kind_name(kind)), error))?;

    observe(engine, &workload, kind, action).await
}

async fn observe<E>(
    engine: &E,
    workload: &OwnedContainer,
    kind: ResourceKind,
    action: WorkloadReconcileAction,
) -> Result<WorkloadReconcileResult, WorkloadReconcileError>
where
    E: HealthObserver,
{
    let health = engine
        .observe_health(workload)
        .await
        .map_err(|error| engine_error(format!("observe {} health", kind_name(kind)), error))?;

    Ok(WorkloadReconcileResult::new(
        workload.clone(),
        action,
        health,
    ))
}

fn validate_request(
    options: &WorkloadReconcileOptions<'_>,
    expected_kind: ResourceKind,
) -> Result<(String, Option<String>), WorkloadReconcileError> {
    let metadata = options.request.metadata();
    let project_id =
        metadata
            .project_id()
            .ok_or_else(|| WorkloadReconcileError::InvalidRequest {
                detail: "project workload reconciliation requires a project owner".to_owned(),
            })?;
    let resource_id = match expected_kind {
        ResourceKind::ProjectApplication
        | ResourceKind::ProjectProcess
        | ResourceKind::ProjectService => Some(
            metadata
                .resource_id()
                .ok_or_else(|| WorkloadReconcileError::InvalidRequest {
                    detail: format!(
                        "{} reconciliation requires a resource identity",
                        kind_name(expected_kind)
                    ),
                })?
                .to_owned(),
        ),
        _ => {
            return Err(WorkloadReconcileError::InvalidRequest {
                detail: "project workload has an unsupported resource identity".to_owned(),
            });
        }
    };
    if metadata.kind() != expected_kind
        || metadata.installation_id() != options.installation_id
        || metadata.schema_version() != options.schema_version
        || metadata.retention() != RetentionClass::Disposable
    {
        return Err(WorkloadReconcileError::InvalidRequest {
            detail: format!(
                "{} ownership does not match the active installation",
                kind_name(expected_kind)
            ),
        });
    }

    Ok((project_id.to_owned(), resource_id))
}

fn duplicate_detail(
    project_id: &str,
    resource_id: Option<&str>,
    kind: ResourceKind,
    count: usize,
) -> String {
    match (kind, resource_id) {
        (ResourceKind::ProjectApplication, Some(resource_id)) => format!(
            "project '{project_id}' application '{resource_id}' owns {count} containers; refusing to guess"
        ),
        (ResourceKind::ProjectProcess, Some(resource_id)) => format!(
            "project '{project_id}' process '{resource_id}' owns {count} containers; refusing to guess"
        ),
        (ResourceKind::ProjectService, Some(resource_id)) => format!(
            "project '{project_id}' service '{resource_id}' owns {count} containers; refusing to guess"
        ),
        _ => format!("project '{project_id}' owns {count} ambiguous workload containers"),
    }
}

const fn kind_name(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::ProjectApplication => "project application",
        ResourceKind::ProjectProcess => "project process",
        ResourceKind::ProjectService => "project service",
        _ => "project workload",
    }
}

fn engine_error(action: impl Into<String>, error: EngineError) -> WorkloadReconcileError {
    WorkloadReconcileError::Engine {
        action: action.into(),
        detail: error.to_string(),
    }
}
