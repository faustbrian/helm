use super::{GatewayError, GatewayReadinessOptions};
use crate::control_plane::engine::{ContainerHealth, HealthObserver};
use tokio::time::Instant;

/// Waits for the owned gateway healthcheck before route configuration begins.
pub(crate) async fn wait_for_gateway_ready(
    observer: &dyn HealthObserver,
    options: GatewayReadinessOptions<'_>,
) -> Result<ContainerHealth, GatewayError> {
    let deadline = Instant::now() + options.timeout();

    loop {
        let health = observer
            .observe_health(options.gateway())
            .await
            .map_err(|error| GatewayError::Engine {
                action: "wait for gateway readiness".to_owned(),
                detail: error.to_string(),
            })?;

        match health {
            ContainerHealth::Healthy => return Ok(health),
            ContainerHealth::Missing
            | ContainerHealth::Stopped
            | ContainerHealth::Unhealthy { .. } => {
                return Err(GatewayError::Reconciliation {
                    detail: format!("gateway cannot become ready from observed health {health:?}"),
                });
            }
            ContainerHealth::RunningUnverified | ContainerHealth::Starting => {}
        }

        let now = Instant::now();
        if now >= deadline {
            return Err(GatewayError::Reconciliation {
                detail: format!(
                    "gateway did not become healthy within {:?}; last observed health: {health:?}",
                    options.timeout()
                ),
            });
        }

        tokio::time::sleep(options.poll_interval().min(deadline - now)).await;
    }
}
