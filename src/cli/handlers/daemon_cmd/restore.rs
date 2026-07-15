use crate::cli::args::DaemonRestoreArgs;
use crate::output::{self, LogLevel, Persistence};
use anyhow::{Context, Result, bail};
use base64::Engine as _;
use serde::Deserialize;
use std::time::{Duration, Instant};

const RESTORE_TIMEOUT: Duration = Duration::from_secs(30 * 60 + 10);
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(50);
const AWAITING_CONFIRMATION: &str = "awaiting_confirmation";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RestoreEvidence {
    migration_id: String,
    state: String,
}

#[cfg(unix)]
pub(super) fn handle_daemon_restore(args: &DaemonRestoreArgs) -> Result<()> {
    use crate::control_plane::{IpcOutcome, IpcPayload, IpcResult};

    let canonical_path = std::fs::canonicalize(&args.path)?;
    let response = super::send_singleton_request(IpcPayload::RestoreProjectService {
        canonical_path,
        recovery_point_id: args.recovery_point_id.clone(),
    })?;
    let operation_id = match response.outcome() {
        IpcOutcome::Success {
            result: IpcResult::Accepted { operation_id },
        } => operation_id.clone(),
        IpcOutcome::Success { .. } => bail!("daemon returned an unexpected restore response"),
        IpcOutcome::Failure { diagnostics } => {
            bail!(
                "project restore failed: {}",
                diagnostics_message(diagnostics)
            )
        }
    };

    follow_restore(&operation_id)
}

#[cfg(unix)]
fn follow_restore(operation_id: &str) -> Result<()> {
    use crate::control_plane::{IpcEventKind, IpcOutcome, IpcOutputStream, IpcPayload, IpcResult};

    let deadline = Instant::now() + RESTORE_TIMEOUT;
    let mut cursor = None;
    let mut evidence = None;
    loop {
        if Instant::now() >= deadline {
            bail!("timed out waiting for project restore '{operation_id}'");
        }
        let response = super::send_singleton_request(IpcPayload::SubscribeEvents {
            after_sequence: cursor,
        })?;
        let (events, latest_sequence) = match response.outcome() {
            IpcOutcome::Success {
                result:
                    IpcResult::Events {
                        events,
                        latest_sequence,
                    },
            } => (events, *latest_sequence),
            IpcOutcome::Success { .. } => bail!("daemon returned an unexpected event response"),
            IpcOutcome::Failure { diagnostics } => {
                bail!(
                    "restore event stream failed: {}",
                    diagnostics_message(diagnostics)
                )
            }
        };
        cursor = Some(latest_sequence);
        for event in events {
            if event.operation_id() != operation_id {
                continue;
            }
            match event.kind() {
                IpcEventKind::Accepted => {}
                IpcEventKind::Output {
                    stream: IpcOutputStream::Stdout,
                    data_base64,
                } => evidence = Some(decode_evidence(data_base64)?),
                IpcEventKind::Output { .. } => {
                    bail!("project restore returned unexpected stderr output")
                }
                IpcEventKind::Completed => {
                    let evidence = evidence
                        .ok_or_else(|| anyhow::anyhow!("restore completed without evidence"))?;
                    if evidence.migration_id != operation_id
                        || evidence.state != AWAITING_CONFIRMATION
                    {
                        bail!("restore completed with inconsistent migration evidence");
                    }
                    output::event(
                        "daemon",
                        LogLevel::Success,
                        &format!(
                            "Migration {} is awaiting explicit confirmation or rollback",
                            evidence.migration_id,
                        ),
                        Persistence::Persistent,
                    );

                    return Ok(());
                }
                IpcEventKind::Failed { code, message } => {
                    bail!("project restore failed ({code}): {message}")
                }
                IpcEventKind::Diagnostics { .. } => {
                    bail!("project restore returned an unexpected diagnostic snapshot")
                }
                IpcEventKind::Cancelled => bail!("project restore was cancelled"),
            }
        }
        std::thread::sleep(EVENT_POLL_INTERVAL);
    }
}

#[cfg(unix)]
fn decode_evidence(encoded: &str) -> Result<RestoreEvidence> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .context("daemon returned invalid Base64 restore evidence")?;
    serde_json::from_slice(&bytes).context("daemon returned invalid restore evidence")
}

#[cfg(unix)]
fn diagnostics_message(diagnostics: &[crate::control_plane::IpcDiagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| format!("{}: {}", diagnostic.code(), diagnostic.message()))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::decode_evidence;
    #[cfg(unix)]
    use base64::Engine as _;

    #[cfg(unix)]
    #[test]
    fn restore_evidence_requires_the_complete_typed_payload() {
        let encoded = base64::engine::general_purpose::STANDARD
            .encode(br#"{"migration_id":"restore-42","state":"awaiting_confirmation"}"#);

        let evidence = decode_evidence(&encoded).expect("typed restore evidence");

        assert_eq!(evidence.migration_id, "restore-42");
        assert_eq!(evidence.state, "awaiting_confirmation");
        assert!(decode_evidence("not-base64").is_err());
    }
}
