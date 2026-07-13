use super::{
    GatewayConfiguration, GatewayError, GatewayPlaneOptions, GatewayPlaneResult,
    GatewayReadinessOptions, reconcile_gateway, reconcile_gateway_configuration,
    wait_for_gateway_ready,
};
use crate::control_plane::engine::{
    ContainerDiscovery, ContainerHealth, ContainerLifecycle, HealthObserver, PublishedPortDiscovery,
};

/// Reconciles gateway ownership, readiness, and complete routes in safe order.
pub(crate) async fn reconcile_gateway_plane<E>(
    engine: &mut E,
    provider: &mut dyn GatewayConfiguration,
    options: GatewayPlaneOptions<'_>,
) -> Result<GatewayPlaneResult, GatewayError>
where
    E: ContainerDiscovery + ContainerLifecycle + HealthObserver + PublishedPortDiscovery,
{
    let gateway = reconcile_gateway(engine, options.gateway).await?;
    let health = if gateway.health() == ContainerHealth::Healthy {
        ContainerHealth::Healthy
    } else {
        wait_for_gateway_ready(
            engine,
            GatewayReadinessOptions::new(
                gateway.container(),
                options.readiness_timeout,
                options.readiness_poll_interval,
            )?,
        )
        .await?
    };
    let configuration_action = reconcile_gateway_configuration(provider, options.snapshot).await?;

    Ok(GatewayPlaneResult::new(
        gateway.action(),
        health,
        configuration_action,
    ))
}
