use super::{
    GatewayError,
    store_active_gateway_certificate_generation::ACTIVE_GATEWAY_CERTIFICATE_GENERATION_FILE,
};
use std::path::Path;
use std::time::{Duration, Instant};

/// Waits until successful gateway reconciliation publishes the expected generation.
pub(crate) fn wait_for_gateway_certificate_generation(
    runtime_directory: &Path,
    expected: &str,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<(), GatewayError> {
    if timeout.is_zero() || poll_interval.is_zero() {
        return Err(GatewayError::InvalidPlan {
            detail: "gateway certificate activation timing must be greater than zero".to_owned(),
        });
    }
    let path = runtime_directory
        .join("gateway")
        .join(ACTIVE_GATEWAY_CERTIFICATE_GENERATION_FILE);
    let started = Instant::now();
    loop {
        match std::fs::read_to_string(&path) {
            Ok(generation) if generation.trim() == expected => return Ok(()),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(GatewayError::Reconciliation {
                    detail: format!(
                        "failed to read active gateway certificate generation '{}': {error}",
                        path.display()
                    ),
                });
            }
        }
        if started.elapsed() >= timeout {
            return Err(GatewayError::Reconciliation {
                detail: format!(
                    "gateway did not activate certificate generation {expected} within {}ms",
                    timeout.as_millis()
                ),
            });
        }
        std::thread::sleep(poll_interval);
    }
}
