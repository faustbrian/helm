use crate::control_plane::{
    IpcOutcome, IpcPayload, IpcRequest, IpcResult, default_unix_daemon_runtime_directory,
    send_unix_request,
};
use anyhow::{Result, bail};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const READINESS_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_millis(250);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Verifies that the activated singleton accepts the current IPC protocol.
pub(super) fn verify() -> Result<()> {
    let socket_path = default_unix_daemon_runtime_directory()?.join("daemon.sock");
    wait_for_readiness(READINESS_TIMEOUT, POLL_INTERVAL, || {
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
        match response.outcome() {
            IpcOutcome::Success {
                result: IpcResult::Pong,
            } => Ok(()),
            outcome => bail!("unexpected daemon readiness response: {outcome:?}"),
        }
    })
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
    use super::wait_for_readiness;
    use anyhow::bail;
    use std::cell::Cell;
    use std::time::Duration;

    #[test]
    fn readiness_retries_transient_failures_until_ping_succeeds() {
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
    fn readiness_timeout_preserves_the_last_probe_error() {
        let error = wait_for_readiness(Duration::from_millis(2), Duration::from_millis(1), || {
            anyhow::bail!("protocol mismatch")
        })
        .expect_err("bounded readiness failure");

        assert!(error.to_string().contains("within 2ms"));
        assert!(error.to_string().contains("protocol mismatch"));
    }
}
