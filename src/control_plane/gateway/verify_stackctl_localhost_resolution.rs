use super::{GatewayError, LocalhostResolver};

const PROBE_HOST: &str = "stackctl-probe.stackctl.localhost";
const RECOVERY: &str = "Stackctl requires the OS .localhost namespace to resolve Stackctl domains to loopback and will not edit /etc/hosts; restore standard .localhost resolution, then rerun `stackctl setup`";

/// Fails setup when the platform does not keep Stackctl routes on loopback.
pub(crate) fn verify_stackctl_localhost_resolution(
    resolver: &impl LocalhostResolver,
) -> Result<(), GatewayError> {
    let addresses = resolver
        .resolve(PROBE_HOST)
        .map_err(|error| preflight(format!("failed to resolve {PROBE_HOST}: {error}")))?;

    if addresses.is_empty() {
        return Err(preflight(format!(
            "{PROBE_HOST} did not resolve to a loopback address"
        )));
    }

    if let Some(address) = addresses.iter().find(|address| !address.is_loopback()) {
        return Err(preflight(format!(
            "{PROBE_HOST} resolved to non-loopback address {address}"
        )));
    }

    Ok(())
}

fn preflight(detail: String) -> GatewayError {
    GatewayError::Preflight {
        detail: format!("{detail}; {RECOVERY}"),
    }
}
