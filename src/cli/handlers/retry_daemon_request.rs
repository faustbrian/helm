use crate::control_plane::{IpcOutcome, IpcResponse};
use anyhow::{Result, bail};
use std::time::Duration;

pub(super) fn retry_daemon_request(
    attempts: usize,
    interval: Duration,
    mut request: impl FnMut() -> Result<IpcResponse>,
) -> Result<IpcResponse> {
    if attempts == 0 {
        bail!("daemon request retry attempts must be greater than zero");
    }
    let mut last_diagnostics = String::new();
    for attempt in 0..attempts {
        let response = request()?;
        let retryable = match response.outcome() {
            IpcOutcome::Failure { diagnostics }
                if !diagnostics.is_empty()
                    && diagnostics.iter().all(|diagnostic| diagnostic.retryable()) =>
            {
                last_diagnostics = diagnostics
                    .iter()
                    .map(|diagnostic| format!("{}: {}", diagnostic.code(), diagnostic.message()))
                    .collect::<Vec<_>>()
                    .join("; ");
                true
            }
            _ => false,
        };
        if !retryable {
            return Ok(response);
        }
        if attempt + 1 < attempts && !interval.is_zero() {
            std::thread::sleep(interval);
        }
    }

    bail!("daemon request did not become ready: {last_diagnostics}")
}

#[cfg(test)]
mod tests {
    use super::retry_daemon_request;
    use crate::control_plane::{IpcDiagnostic, IpcResponse, IpcResult};
    use std::time::Duration;

    #[test]
    fn retries_retryable_daemon_rejections_until_accepted() {
        let mut attempts = 0;

        let response = retry_daemon_request(3, Duration::ZERO, || {
            attempts += 1;
            Ok(if attempts < 3 {
                IpcResponse::failure(
                    "request",
                    vec![IpcDiagnostic::new("project_not_ready", "starting", true)],
                )
            } else {
                IpcResponse::success(
                    "request",
                    IpcResult::Accepted {
                        operation_id: "request".to_owned(),
                    },
                )
            })
        })
        .expect("eventual acceptance");

        assert_eq!(attempts, 3);
        assert!(matches!(
            response.outcome(),
            crate::control_plane::IpcOutcome::Success {
                result: IpcResult::Accepted { .. }
            }
        ));
    }

    #[test]
    fn does_not_retry_a_permanent_daemon_rejection() {
        let mut attempts = 0;

        let response = retry_daemon_request(3, Duration::ZERO, || {
            attempts += 1;
            Ok(IpcResponse::failure(
                "request",
                vec![IpcDiagnostic::new(
                    "project_invalid",
                    "configuration is invalid",
                    false,
                )],
            ))
        })
        .expect("permanent response");

        assert_eq!(attempts, 1);
        assert!(matches!(
            response.outcome(),
            crate::control_plane::IpcOutcome::Failure { .. }
        ));
    }
}
