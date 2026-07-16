use super::daemon_readiness_error::DaemonReadinessError;
use crate::control_plane::{
    IpcOutcome, IpcPayload, IpcRequest, IpcResult, default_unix_daemon_runtime_directory,
    send_unix_request,
};
use anyhow::{Error, Result, bail};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg_attr(test, allow(dead_code))]
const READINESS_TIMEOUT: Duration = Duration::from_secs(300);
#[cfg_attr(test, allow(dead_code))]
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg_attr(test, allow(dead_code))]
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Verifies that the activated singleton has converged its initial desired state.
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
        &IpcRequest::new(request_id, IpcPayload::DaemonStatus),
        PROBE_TIMEOUT,
    )?;
    validate_response(response.outcome())
}

fn validate_response(outcome: &IpcOutcome) -> Result<()> {
    match outcome {
        IpcOutcome::Success {
            result:
                IpcResult::DaemonStatus {
                    discovery_complete: true,
                    engine_available: true,
                    engine_converged: true,
                    discovery_diagnostics,
                    reconciliation_diagnostic: None,
                },
        } if discovery_diagnostics.is_empty() => Ok(()),
        IpcOutcome::Success {
            result:
                IpcResult::DaemonStatus {
                    discovery_complete,
                    engine_available,
                    engine_converged,
                    discovery_diagnostics,
                    reconciliation_diagnostic,
                },
        } => {
            let mut issues = Vec::new();
            if !discovery_complete {
                issues.push("initial project discovery is incomplete".to_owned());
            }
            if !engine_available {
                issues.push("selected container Engine is unavailable".to_owned());
            }
            if !engine_converged {
                issues.push("managed services, gateway, or routes have not converged".to_owned());
            }
            issues.extend(
                discovery_diagnostics
                    .iter()
                    .map(|diagnostic| format!("{}: {}", diagnostic.code(), diagnostic.message())),
            );
            if let Some(diagnostic) = reconciliation_diagnostic {
                issues.push(format!("{}: {}", diagnostic.code(), diagnostic.message()));
            }
            let retryable = discovery_diagnostics
                .iter()
                .all(|diagnostic| diagnostic.retryable())
                && reconciliation_diagnostic
                    .as_ref()
                    .is_none_or(|diagnostic| diagnostic.retryable());

            Err(Error::new(DaemonReadinessError::new(
                format!("daemon is not operational: {}", issues.join("; ")),
                retryable,
            )))
        }
        IpcOutcome::Failure { diagnostics } => Err(Error::new(DaemonReadinessError::new(
            format!("daemon readiness failed: {diagnostics:?}"),
            diagnostics.iter().all(|diagnostic| diagnostic.retryable()),
        ))),
        outcome => Err(Error::new(DaemonReadinessError::new(
            format!("unexpected daemon readiness response: {outcome:?}"),
            false,
        ))),
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
        if error
            .downcast_ref::<DaemonReadinessError>()
            .is_some_and(|error| !error.retryable())
        {
            return Err(error);
        }
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
    fn readiness_retries_transient_failures_until_daemon_is_operational() {
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
    fn readiness_rejects_a_responsive_but_unconverged_daemon() {
        let diagnostic = IpcDiagnostic::new(
            "project_adoption_required",
            "project 'api' has disabled managed state; explicit adoption is required",
            false,
        );
        let outcome = IpcOutcome::Success {
            result: IpcResult::DaemonStatus {
                discovery_complete: true,
                engine_available: true,
                engine_converged: false,
                discovery_diagnostics: Vec::new(),
                reconciliation_diagnostic: Some(diagnostic),
            },
        };

        let error = validate_response(&outcome).expect_err("unconverged daemon must fail");

        assert!(error.to_string().contains("gateway"));
        assert!(error.to_string().contains("have not converged"));
        assert!(error.to_string().contains("project_adoption_required"));
        assert!(error.to_string().contains("explicit adoption is required"));
    }

    #[test]
    fn readiness_does_not_retry_non_retryable_diagnostics() {
        let attempts = Cell::new(0_u8);
        let error = wait_for_readiness(Duration::from_millis(20), Duration::from_millis(1), || {
            attempts.set(attempts.get() + 1);
            validate_response(&IpcOutcome::Success {
                result: IpcResult::DaemonStatus {
                    discovery_complete: true,
                    engine_available: true,
                    engine_converged: false,
                    discovery_diagnostics: Vec::new(),
                    reconciliation_diagnostic: Some(IpcDiagnostic::new(
                        "project_adoption_required",
                        "run `stackctl daemon adopt /workspace/api`",
                        false,
                    )),
                },
            })
        })
        .expect_err("non-retryable readiness diagnostic");

        assert_eq!(attempts.get(), 1);
        assert!(error.to_string().contains("project_adoption_required"));
        assert!(error.to_string().contains("stackctl daemon adopt"));
    }

    #[test]
    fn readiness_preserves_structured_discovery_diagnostics() {
        let outcome = IpcOutcome::Success {
            result: IpcResult::DaemonStatus {
                discovery_complete: false,
                engine_available: true,
                engine_converged: false,
                discovery_diagnostics: vec![IpcDiagnostic::new(
                    "configuration_collision",
                    "domain 'api-app.stackctl.localhost' has multiple claimants",
                    false,
                )],
                reconciliation_diagnostic: None,
            },
        };

        let error = validate_response(&outcome).expect_err("invalid discovery must fail");

        assert!(
            error
                .to_string()
                .contains("initial project discovery is incomplete")
        );
        assert!(error.to_string().contains("configuration_collision"));
        assert!(error.to_string().contains("multiple claimants"));
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
