use super::{GatewayConfiguration, GatewayConfigurationAction, GatewayError, GatewaySnapshot};

/// Applies and verifies one complete desired gateway route revision.
pub(crate) async fn reconcile_gateway_configuration(
    provider: &mut dyn GatewayConfiguration,
    snapshot: &GatewaySnapshot,
) -> Result<GatewayConfigurationAction, GatewayError> {
    let active_revision = provider.active_revision().await?;
    if active_revision.as_deref() == Some(snapshot.revision()) {
        return Ok(GatewayConfigurationAction::Unchanged);
    }

    provider.apply_snapshot(snapshot).await?;

    let active_revision = provider.active_revision().await?;
    if active_revision.as_deref() != Some(snapshot.revision()) {
        return Err(GatewayError::Reconciliation {
            detail: format!(
                "gateway applied revision '{}' but reported active revision '{}'",
                snapshot.revision(),
                active_revision.as_deref().unwrap_or("<none>")
            ),
        });
    }

    Ok(GatewayConfigurationAction::Applied)
}
