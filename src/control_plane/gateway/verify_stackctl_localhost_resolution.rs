use super::{GatewayError, LocalhostResolver};

const PROBE_HOST: &str = "stackctl-probe.stackctl.localhost";

/// Fails setup when the platform does not keep Stackctl routes on loopback.
pub(crate) fn verify_stackctl_localhost_resolution(
    resolver: &impl LocalhostResolver,
) -> Result<(), GatewayError> {
    let addresses = resolver.resolve(PROBE_HOST)?;

    if addresses.is_empty() {
        return Err(GatewayError::Provider {
            detail: format!("{PROBE_HOST} did not resolve to a loopback address"),
        });
    }

    if let Some(address) = addresses.iter().find(|address| !address.is_loopback()) {
        return Err(GatewayError::Provider {
            detail: format!("{PROBE_HOST} resolved to non-loopback address {address}"),
        });
    }

    Ok(())
}
