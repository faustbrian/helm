use crate::control_plane::{
    IpcOutcome, IpcPayload, IpcRequest, IpcResult, default_unix_daemon_runtime_directory,
    send_unix_request,
};
use anyhow::{Result, bail};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg_attr(test, allow(dead_code))]
const READINESS_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg_attr(test, allow(dead_code))]
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg_attr(test, allow(dead_code))]
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Verifies that the activated singleton owns and serves its IPC endpoint.
#[cfg_attr(test, allow(dead_code))]
pub(super) fn verify() -> Result<()> {
    wait_for_readiness(READINESS_TIMEOUT, POLL_INTERVAL, probe)
}

/// Performs one bounded correlated readiness probe without retrying.
#[cfg_attr(test, allow(dead_code))]
pub(super) fn probe() -> Result<()> {
    let socket_path = default_unix_daemon_runtime_directory()?.join("daemon.sock");
    let request_id = format!(
        "service-readiness-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    );
    let response = send_unix_request(
        &socket_path,
        &IpcRequest::new(request_id, IpcPayload::Ping),
        PROBE_TIMEOUT,
    )?;
    validate_response(response.outcome())
}

fn validate_response(outcome: &IpcOutcome) -> Result<()> {
    match outcome {
        IpcOutcome::Success {
            result: IpcResult::Pong,
        } => Ok(()),
        outcome => bail!("unexpected daemon readiness response: {outcome:?}"),
    }
}

fn wait_for_readiness(
    timeout: Duration,
    poll_interval: Duration,
    mut probe: impl FnMut() -> Result<()>,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        let error = match probe() {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };
        let now = Instant::now();
        if now >= deadline {
            bail!("daemon did not become IPC-ready within {timeout:?}: {error}");
        }
        thread::sleep(poll_interval.min(deadline.saturating_duration_since(now)));
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_response, wait_for_readiness};
    use crate::control_plane::{IpcDiagnostic, IpcOutcome, IpcResult};
    use anyhow::bail;
    use std::cell::Cell;
    use std::time::Duration;

    #[test]
    fn readiness_retries_transient_failures_until_daemon_responds() {
        let attempts = Cell::new(0_u8);

        wait_for_readiness(Duration::from_millis(20), Duration::from_millis(1), || {
            attempts.set(attempts.get() + 1);
            if attempts.get() < 3 {
                bail!("socket not ready")
            }
            Ok(())
        })
        .expect("eventual readiness");

        assert_eq!(attempts.get(), 3);
    }

    #[test]
    fn readiness_accepts_a_responsive_daemon_before_projects_converge() {
        validate_response(&IpcOutcome::Success {
            result: IpcResult::Pong,
        })
        .expect("responsive daemon");
    }

    #[test]
    fn readiness_rejects_project_status_as_the_service_handshake() {
        let outcome = IpcOutcome::Success {
            result: IpcResult::DaemonStatus {
                discovery_complete: false,
                engine_available: false,
                engine_converged: false,
                discovery_diagnostics: vec![IpcDiagnostic::new(
                    "configuration_collision",
                    "multiple claimants",
                    false,
                )],
                reconciliation_diagnostic: None,
            },
        };

        let error = validate_response(&outcome).expect_err("status is not a pong");

        assert!(
            error
                .to_string()
                .contains("unexpected daemon readiness response")
        );
    }

    #[test]
    fn readiness_timeout_preserves_the_last_probe_error() {
        let error = wait_for_readiness(Duration::from_millis(2), Duration::from_millis(1), || {
            anyhow::bail!("protocol mismatch")
        })
        .expect_err("bounded readiness failure");

        assert!(error.to_string().contains("within 2ms"));
        assert!(error.to_string().contains("protocol mismatch"));
    }
}
