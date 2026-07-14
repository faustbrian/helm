use crate::cli::args::DaemonBackupArgs;
use crate::output::{self, LogLevel, Persistence};
use anyhow::{Context, Result, bail};
use base64::Engine as _;
use serde::Deserialize;
use std::time::{Duration, Instant};

const BACKUP_TIMEOUT: Duration = Duration::from_secs(10 * 60 + 10);
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BackupEvidence {
    artifact_sha256: String,
    artifact_size_bytes: u64,
    recovery_point: String,
}

#[cfg(unix)]
pub(super) fn handle_daemon_backup(args: &DaemonBackupArgs) -> Result<()> {
    use crate::control_plane::{IpcOutcome, IpcPayload, IpcResult};

    let canonical_path = std::fs::canonicalize(&args.path)?;
    let response = super::send_singleton_request(IpcPayload::BackupProjectService {
        canonical_path,
        service: args.service.clone(),
    })?;
    let operation_id = match response.outcome() {
        IpcOutcome::Success {
            result: IpcResult::Accepted { operation_id },
        } => operation_id.clone(),
        IpcOutcome::Success { .. } => bail!("daemon returned an unexpected backup response"),
        IpcOutcome::Failure { diagnostics } => {
            bail!(
                "project backup failed: {}",
                diagnostics_message(diagnostics)
            )
        }
    };

    follow_backup(&operation_id)
}

#[cfg(unix)]
fn follow_backup(operation_id: &str) -> Result<()> {
    use crate::control_plane::{IpcEventKind, IpcOutcome, IpcOutputStream, IpcPayload, IpcResult};

    let deadline = Instant::now() + BACKUP_TIMEOUT;
    let mut cursor = None;
    let mut evidence = None;
    loop {
        if Instant::now() >= deadline {
            bail!("timed out waiting for project backup '{operation_id}'");
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
                    "backup event stream failed: {}",
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
                    bail!("project backup returned unexpected stderr output")
                }
                IpcEventKind::Completed => {
                    let evidence = evidence
                        .ok_or_else(|| anyhow::anyhow!("backup completed without evidence"))?;
                    output::event(
                        "daemon",
                        LogLevel::Success,
                        &format!(
                            "Recovery point {} ({} bytes, {})",
                            evidence.recovery_point,
                            evidence.artifact_size_bytes,
                            evidence.artifact_sha256,
                        ),
                        Persistence::Persistent,
                    );

                    return Ok(());
                }
                IpcEventKind::Failed { code, message } => {
                    bail!("project backup failed ({code}): {message}")
                }
                IpcEventKind::Cancelled => bail!("project backup was cancelled"),
            }
        }
        std::thread::sleep(EVENT_POLL_INTERVAL);
    }
}

#[cfg(unix)]
fn decode_evidence(encoded: &str) -> Result<BackupEvidence> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .context("daemon returned invalid Base64 backup evidence")?;
    serde_json::from_slice(&bytes).context("daemon returned invalid backup evidence")
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
    fn backup_evidence_requires_the_complete_typed_payload() {
        let encoded = base64::engine::general_purpose::STANDARD.encode(
            br#"{"artifact_sha256":"sha256:abc","artifact_size_bytes":42,"recovery_point":"/state/backups/42"}"#,
        );

        let evidence = decode_evidence(&encoded).expect("typed backup evidence");

        assert_eq!(evidence.artifact_sha256, "sha256:abc");
        assert_eq!(evidence.artifact_size_bytes, 42);
        assert_eq!(evidence.recovery_point, "/state/backups/42");
        assert!(decode_evidence("not-base64").is_err());
    }
}
