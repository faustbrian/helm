use super::{
    SharedInfrastructureReconcileError, SharedServiceReconcileAction,
    SharedServiceReconcileOptions, SharedServiceReconcileResult, SharedVolumeReconcileOptions,
    SharedVolumeReconcileResult, reconcile_shared_volume,
};
use crate::control_plane::engine::{
    ContainerDiscovery, ContainerHealth, ContainerLifecycle, ContainerState, EngineError,
    HealthObserver, ObservedResourceOwnership, OwnedContainer, ResourceKind, VolumeDiscovery,
    VolumeManager, reconstruct_owned_container,
};

const FINGERPRINT_LABEL: &str = "dev.stackctl.fingerprint";
const KIND_LABEL: &str = "dev.stackctl.kind";
const SHARED_SERVICE_KIND: &str = "shared_service";

/// Converges one compatibility-keyed process while retaining its data volume.
pub(crate) async fn reconcile_shared_service<E>(
    engine: &mut E,
    options: SharedServiceReconcileOptions<'_>,
) -> Result<SharedServiceReconcileResult, SharedInfrastructureReconcileError>
where
    E: ContainerDiscovery + ContainerLifecycle + HealthObserver + VolumeDiscovery + VolumeManager,
{
    validate_request(&options)?;
    let desired_metadata = options.request.metadata();
    let fingerprint = desired_metadata.compatibility_fingerprint();
    let observed = engine
        .discover_managed()
        .await
        .map_err(|error| engine_error("container discovery", error))?;
    let mut matching = Vec::new();

    for container in observed.iter().filter(|container| {
        container.labels().get(KIND_LABEL).map(String::as_str) == Some(SHARED_SERVICE_KIND)
            && container
                .labels()
                .get(FINGERPRINT_LABEL)
                .map(String::as_str)
                == Some(fingerprint)
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
                        "shared service compatibility '{fingerprint}' is already owned by another installation"
                    ),
                });
            }
            Err(ownership) => {
                return Err(SharedInfrastructureReconcileError::Conflict {
                    detail: format!(
                        "shared service compatibility '{fingerprint}' has invalid ownership: {ownership:?}"
                    ),
                });
            }
        }
    }

    let existing = match matching.as_slice() {
        [] => None,
        [container] => Some(container.clone()),
        containers => {
            return Err(SharedInfrastructureReconcileError::Conflict {
                detail: format!(
                    "shared service compatibility '{fingerprint}' owns {} containers; refusing to guess",
                    containers.len()
                ),
            });
        }
    };
    let volume = reconcile_volume(engine, &options).await?;

    match existing {
        None => {
            create_service(
                engine,
                options.request,
                volume,
                SharedServiceReconcileAction::Created,
            )
            .await
        }
        Some(container) if container.metadata() != desired_metadata => {
            replace_service(engine, options.request, volume, &container).await
        }
        Some(container) => reconcile_existing(engine, options.request, volume, &container).await,
    }
}

async fn reconcile_volume<E>(
    engine: &mut E,
    options: &SharedServiceReconcileOptions<'_>,
) -> Result<Option<SharedVolumeReconcileResult>, SharedInfrastructureReconcileError>
where
    E: VolumeDiscovery + VolumeManager,
{
    let Some(volume) = options.volume else {
        return Ok(None);
    };

    reconcile_shared_volume(
        engine,
        SharedVolumeReconcileOptions {
            request: volume,
            installation_id: options.installation_id,
            schema_version: options.schema_version,
        },
    )
    .await
    .map(Some)
}

async fn reconcile_existing<E>(
    engine: &mut E,
    request: &crate::control_plane::engine::ContainerCreateOptions,
    volume: Option<SharedVolumeReconcileResult>,
    container: &OwnedContainer,
) -> Result<SharedServiceReconcileResult, SharedInfrastructureReconcileError>
where
    E: ContainerLifecycle + HealthObserver,
{
    match engine
        .inspect(container)
        .await
        .map_err(|error| engine_error("container inspection", error))?
    {
        ContainerState::Missing => {
            create_service(
                engine,
                request,
                volume,
                SharedServiceReconcileAction::Created,
            )
            .await
        }
        ContainerState::Stopped => {
            engine
                .start(container)
                .await
                .map_err(|error| engine_error("container start", error))?;
            observe(
                engine,
                container.clone(),
                volume,
                SharedServiceReconcileAction::Started,
            )
            .await
        }
        ContainerState::Running => match engine
            .observe_health(container)
            .await
            .map_err(|error| engine_error("health observation", error))?
        {
            ContainerHealth::Unhealthy { .. } => {
                engine
                    .stop(container)
                    .await
                    .map_err(|error| engine_error("unhealthy container stop", error))?;
                engine
                    .start(container)
                    .await
                    .map_err(|error| engine_error("unhealthy container restart", error))?;
                observe(
                    engine,
                    container.clone(),
                    volume,
                    SharedServiceReconcileAction::Restarted,
                )
                .await
            }
            health => Ok(SharedServiceReconcileResult::new(
                container.clone(),
                volume,
                SharedServiceReconcileAction::Unchanged,
                health,
            )),
        },
    }
}

async fn replace_service<E>(
    engine: &mut E,
    request: &crate::control_plane::engine::ContainerCreateOptions,
    volume: Option<SharedVolumeReconcileResult>,
    container: &OwnedContainer,
) -> Result<SharedServiceReconcileResult, SharedInfrastructureReconcileError>
where
    E: ContainerLifecycle + HealthObserver,
{
    match engine
        .inspect(container)
        .await
        .map_err(|error| engine_error("drifted container inspection", error))?
    {
        ContainerState::Running => engine
            .stop(container)
            .await
            .map_err(|error| engine_error("drifted container stop", error))?,
        ContainerState::Stopped => {}
        ContainerState::Missing => {
            return create_service(
                engine,
                request,
                volume,
                SharedServiceReconcileAction::Created,
            )
            .await;
        }
    }
    engine
        .remove(container)
        .await
        .map_err(|error| engine_error("drifted container removal", error))?;

    create_service(
        engine,
        request,
        volume,
        SharedServiceReconcileAction::Replaced,
    )
    .await
}

async fn create_service<E>(
    engine: &mut E,
    request: &crate::control_plane::engine::ContainerCreateOptions,
    volume: Option<SharedVolumeReconcileResult>,
    action: SharedServiceReconcileAction,
) -> Result<SharedServiceReconcileResult, SharedInfrastructureReconcileError>
where
    E: ContainerLifecycle + HealthObserver,
{
    let container = engine
        .create(request)
        .await
        .map_err(|error| engine_error("container creation", error))?;
    if container.metadata() != request.metadata() {
        return Err(SharedInfrastructureReconcileError::Engine {
            action: "container creation".to_owned(),
            detail: "Engine returned a shared service with unexpected ownership".to_owned(),
        });
    }
    engine
        .start(&container)
        .await
        .map_err(|error| engine_error("container start", error))?;
    observe(engine, container, volume, action).await
}

async fn observe<E>(
    engine: &E,
    container: OwnedContainer,
    volume: Option<SharedVolumeReconcileResult>,
    action: SharedServiceReconcileAction,
) -> Result<SharedServiceReconcileResult, SharedInfrastructureReconcileError>
where
    E: HealthObserver,
{
    let health = engine
        .observe_health(&container)
        .await
        .map_err(|error| engine_error("health observation", error))?;

    Ok(SharedServiceReconcileResult::new(
        container, volume, action, health,
    ))
}

fn validate_request(
    options: &SharedServiceReconcileOptions<'_>,
) -> Result<(), SharedInfrastructureReconcileError> {
    let metadata = options.request.metadata();
    if metadata.kind() != ResourceKind::SharedService
        || metadata.project_id().is_some()
        || metadata.installation_id() != options.installation_id
        || metadata.schema_version() != options.schema_version
    {
        return Err(SharedInfrastructureReconcileError::InvalidRequest {
            detail: "shared service ownership does not match the active installation".to_owned(),
        });
    }
    if let Some(volume) = options.volume {
        let volume_metadata = volume.metadata();
        if volume_metadata.kind() != ResourceKind::Volume
            || volume_metadata.project_id().is_some()
            || volume_metadata.installation_id() != options.installation_id
            || volume_metadata.schema_version() != options.schema_version
            || volume_metadata.compatibility_fingerprint() != metadata.compatibility_fingerprint()
        {
            return Err(SharedInfrastructureReconcileError::InvalidRequest {
                detail: "shared service volume does not match its compatibility identity"
                    .to_owned(),
            });
        }
    }

    Ok(())
}

fn engine_error(action: &str, error: EngineError) -> SharedInfrastructureReconcileError {
    SharedInfrastructureReconcileError::Engine {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}
