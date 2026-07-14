use crate::cli::args::DaemonMigrationDecisionArgs;
use crate::control_plane::IpcMigrationDecision;
use crate::output::{self, LogLevel, Persistence};
use anyhow::{Context, Result, bail};
use base64::Engine as _;
use serde::Deserialize;
use std::time::{Duration, Instant};

const DECISION_TIMEOUT: Duration = Duration::from_secs(30 * 60 + 10);
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MigrationDecisionEvidence {
    migration_id: String,
    state: String,
}

#[cfg(unix)]
pub(super) fn handle_migration_decision(
    args: &DaemonMigrationDecisionArgs,
    decision: IpcMigrationDecision,
) -> Result<()> {
    use crate::control_plane::{IpcOutcome, IpcPayload, IpcResult};

    let canonical_path = std::fs::canonicalize(&args.path)?;
    let response = super::send_singleton_request(IpcPayload::DecideProjectMigration {
        canonical_path,
        migration_id: args.migration_id.clone(),
        decision,
    })?;
    let operation_id = match response.outcome() {
        IpcOutcome::Success {
            result: IpcResult::Accepted { operation_id },
        } => operation_id.clone(),
        IpcOutcome::Success { .. } => {
            bail!("daemon returned an unexpected migration decision response")
        }
        IpcOutcome::Failure { diagnostics } => {
            bail!(
                "migration {} failed: {}",
                decision.as_str(),
                diagnostics_message(diagnostics)
            )
        }
    };

    follow_decision(&operation_id, &args.migration_id, decision)
}

#[cfg(unix)]
fn follow_decision(
    operation_id: &str,
    migration_id: &str,
    decision: IpcMigrationDecision,
) -> Result<()> {
    use crate::control_plane::{IpcEventKind, IpcOutcome, IpcOutputStream, IpcPayload, IpcResult};

    let deadline = Instant::now() + DECISION_TIMEOUT;
    let mut cursor = None;
    let mut evidence = None;
    loop {
        if Instant::now() >= deadline {
            bail!(
                "timed out waiting for migration {} operation '{operation_id}'",
                decision.as_str()
            );
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
                    "migration decision event stream failed: {}",
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
                    bail!("migration decision returned unexpected stderr output")
                }
                IpcEventKind::Completed => {
                    verify_evidence(evidence.as_ref(), migration_id, decision)?;
                    output::event(
                        "daemon",
                        LogLevel::Success,
                        &format!(
                            "Migration {migration_id} was {}",
                            expected_state(decision).replace('_', " ")
                        ),
                        Persistence::Persistent,
                    );

                    return Ok(());
                }
                IpcEventKind::Failed { code, message } => {
                    bail!("migration {} failed ({code}): {message}", decision.as_str())
                }
                IpcEventKind::Cancelled => {
                    bail!("migration {} was cancelled", decision.as_str())
                }
            }
        }
        std::thread::sleep(EVENT_POLL_INTERVAL);
    }
}

fn verify_evidence(
    evidence: Option<&MigrationDecisionEvidence>,
    migration_id: &str,
    decision: IpcMigrationDecision,
) -> Result<()> {
    let evidence =
        evidence.ok_or_else(|| anyhow::anyhow!("migration decision completed without evidence"))?;
    if evidence.migration_id != migration_id || evidence.state != expected_state(decision) {
        bail!("migration decision completed with inconsistent evidence");
    }

    Ok(())
}

const fn expected_state(decision: IpcMigrationDecision) -> &'static str {
    match decision {
        IpcMigrationDecision::Confirm => "confirmed",
        IpcMigrationDecision::Rollback => "rolled_back",
    }
}

#[cfg(unix)]
fn decode_evidence(encoded: &str) -> Result<MigrationDecisionEvidence> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .context("daemon returned invalid Base64 migration decision evidence")?;
    serde_json::from_slice(&bytes).context("daemon returned invalid migration decision evidence")
}

#[cfg(unix)]
fn diagnostics_message(diagnostics: &[crate::control_plane::IpcDiagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| format!("{}: {}", diagnostic.code(), diagnostic.message()))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(all(test, unix))]
mod tests {
    use super::{decode_evidence, verify_evidence};
    use crate::control_plane::IpcMigrationDecision;
    use base64::Engine as _;

    #[test]
    fn migration_decision_evidence_requires_exact_identity_and_state() {
        let encoded = base64::engine::general_purpose::STANDARD
            .encode(br#"{"migration_id":"restore-42","state":"confirmed"}"#);
        let evidence = decode_evidence(&encoded).expect("typed migration decision evidence");

        verify_evidence(Some(&evidence), "restore-42", IpcMigrationDecision::Confirm)
            .expect("consistent evidence");
        assert!(
            verify_evidence(
                Some(&evidence),
                "restore-42",
                IpcMigrationDecision::Rollback,
            )
            .is_err()
        );
        assert!(decode_evidence("not-base64").is_err());
    }
}
