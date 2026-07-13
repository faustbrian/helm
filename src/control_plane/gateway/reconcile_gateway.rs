use super::{
    GatewayError, GatewayReconcileAction, GatewayReconcileOptions, GatewayReconcileResult,
    preflight_gateway_ports, preflight_gateway_ports::preflight_gateway_ports_ignoring,
};
use crate::control_plane::engine::{
    ContainerDiscovery, ContainerHealth, ContainerLifecycle, ContainerState, HealthObserver,
    ImageResolver, ImmutableImageReference, ObservedResourceOwnership, OwnedContainer,
    PublishedPortDiscovery, ResourceKind, reconstruct_owned_container,
};

/// Restores the singleton owned gateway without mutating unrelated containers.
pub(crate) async fn reconcile_gateway<E>(
    engine: &mut E,
    options: GatewayReconcileOptions<'_>,
) -> Result<GatewayReconcileResult, GatewayError>
where
    E: ContainerDiscovery
        + ContainerLifecycle
        + HealthObserver
        + ImageResolver
        + PublishedPortDiscovery,
{
    validate_request(&options)?;
    let image = ImmutableImageReference::new(options.request.image())
        .map_err(|error| engine_error("validate gateway image", error))?;
    engine
        .ensure_image(&image)
        .await
        .map_err(|error| engine_error("resolve gateway image", error))?;

    let observed = engine
        .discover_managed()
        .await
        .map_err(|error| engine_error("discover managed containers", error))?;
    let mut gateways = Vec::new();

    for container in &observed {
        match reconstruct_owned_container(
            container,
            options.installation_id,
            options.schema_version,
        ) {
            Ok(owned) if owned.metadata().kind() == ResourceKind::Gateway => gateways.push(owned),
            Ok(_) | Err(ObservedResourceOwnership::Unmanaged) => {}
            Err(ObservedResourceOwnership::ForeignInstallation { .. }) => {}
            Err(ownership) => {
                return Err(GatewayError::Reconciliation {
                    detail: format!(
                        "cannot reconcile gateway because managed container '{}' has invalid ownership: {ownership:?}",
                        container.id().as_str()
                    ),
                });
            }
        }
    }

    match gateways.as_slice() {
        [] => create_gateway(engine, &options).await,
        [gateway] => reconcile_existing_gateway(engine, &options, gateway).await,
        gateways => Err(GatewayError::Reconciliation {
            detail: format!(
                "cannot reconcile gateway because installation '{}' owns {} gateway containers",
                options.installation_id,
                gateways.len()
            ),
        }),
    }
}

async fn reconcile_existing_gateway<E>(
    engine: &mut E,
    options: &GatewayReconcileOptions<'_>,
    gateway: &OwnedContainer,
) -> Result<GatewayReconcileResult, GatewayError>
where
    E: ContainerLifecycle + HealthObserver + PublishedPortDiscovery,
{
    if gateway.metadata() != options.request.metadata() {
        return replace_gateway(engine, options, gateway).await;
    }

    match engine
        .inspect(gateway)
        .await
        .map_err(|error| engine_error("inspect owned gateway", error))?
    {
        ContainerState::Running => match engine
            .observe_health(gateway)
            .await
            .map_err(|error| engine_error("observe gateway health", error))?
        {
            ContainerHealth::Unhealthy { .. } => restart_gateway(engine, options, gateway).await,
            health => Ok(GatewayReconcileResult::new(
                gateway.clone(),
                GatewayReconcileAction::Unchanged,
                health,
            )),
        },
        ContainerState::Stopped => {
            preflight_gateway_ports(engine, options.host_probe).await?;
            engine
                .start(gateway)
                .await
                .map_err(|error| engine_error("start owned gateway", error))?;
            observe_gateway(engine, gateway, GatewayReconcileAction::Started).await
        }
        ContainerState::Missing => create_gateway(engine, options).await,
    }
}

async fn create_gateway<E>(
    engine: &mut E,
    options: &GatewayReconcileOptions<'_>,
) -> Result<GatewayReconcileResult, GatewayError>
where
    E: ContainerLifecycle + HealthObserver + PublishedPortDiscovery,
{
    preflight_gateway_ports(engine, options.host_probe).await?;
    create_gateway_after_preflight(engine, options, GatewayReconcileAction::Created).await
}

async fn create_gateway_after_preflight<E>(
    engine: &mut E,
    options: &GatewayReconcileOptions<'_>,
    action: GatewayReconcileAction,
) -> Result<GatewayReconcileResult, GatewayError>
where
    E: ContainerLifecycle + HealthObserver,
{
    let gateway = engine
        .create(options.request)
        .await
        .map_err(|error| engine_error("create gateway", error))?;
    engine
        .start(&gateway)
        .await
        .map_err(|error| engine_error("start created gateway", error))?;

    observe_gateway(engine, &gateway, action).await
}

async fn restart_gateway<E>(
    engine: &mut E,
    options: &GatewayReconcileOptions<'_>,
    gateway: &OwnedContainer,
) -> Result<GatewayReconcileResult, GatewayError>
where
    E: ContainerLifecycle + HealthObserver + PublishedPortDiscovery,
{
    preflight_gateway_ports_ignoring(engine, options.host_probe, Some(gateway.id())).await?;
    engine
        .stop(gateway)
        .await
        .map_err(|error| engine_error("stop unhealthy gateway", error))?;
    engine
        .start(gateway)
        .await
        .map_err(|error| engine_error("restart unhealthy gateway", error))?;

    observe_gateway(engine, gateway, GatewayReconcileAction::Restarted).await
}

async fn replace_gateway<E>(
    engine: &mut E,
    options: &GatewayReconcileOptions<'_>,
    gateway: &OwnedContainer,
) -> Result<GatewayReconcileResult, GatewayError>
where
    E: ContainerLifecycle + HealthObserver + PublishedPortDiscovery,
{
    match engine
        .inspect(gateway)
        .await
        .map_err(|error| engine_error("inspect drifted gateway", error))?
    {
        ContainerState::Running => {
            preflight_gateway_ports_ignoring(engine, options.host_probe, Some(gateway.id()))
                .await?;
            engine
                .stop(gateway)
                .await
                .map_err(|error| engine_error("stop drifted gateway", error))?;
        }
        ContainerState::Stopped => preflight_gateway_ports(engine, options.host_probe).await?,
        ContainerState::Missing => return create_gateway(engine, options).await,
    }

    engine
        .remove(gateway)
        .await
        .map_err(|error| engine_error("remove drifted gateway", error))?;
    create_gateway_after_preflight(engine, options, GatewayReconcileAction::Replaced).await
}

async fn observe_gateway<E>(
    engine: &E,
    gateway: &OwnedContainer,
    action: GatewayReconcileAction,
) -> Result<GatewayReconcileResult, GatewayError>
where
    E: HealthObserver,
{
    let health = engine
        .observe_health(gateway)
        .await
        .map_err(|error| engine_error("observe gateway health", error))?;

    Ok(GatewayReconcileResult::new(gateway.clone(), action, health))
}

fn validate_request(options: &GatewayReconcileOptions<'_>) -> Result<(), GatewayError> {
    let metadata = options.request.metadata();
    if metadata.kind() != ResourceKind::Gateway
        || metadata.installation_id() != options.installation_id
        || metadata.schema_version() != options.schema_version
    {
        return Err(GatewayError::InvalidPlan {
            detail: "gateway reconcile request ownership does not match the active installation"
                .to_owned(),
        });
    }

    Ok(())
}

fn engine_error(action: &str, error: crate::control_plane::engine::EngineError) -> GatewayError {
    GatewayError::Engine {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}
