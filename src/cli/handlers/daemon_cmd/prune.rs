use crate::cli::args::{
    DaemonPruneArgs, DaemonPruneCommands, DaemonPruneExecuteArgs, DaemonPrunePlanArgs,
};
use crate::output::{self, LogLevel, Persistence};
use anyhow::{Result, bail};
use std::time::{Duration, Instant};

const PRUNE_TIMEOUT: Duration = Duration::from_secs(3 * 60);
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(50);

pub(super) fn handle_daemon_prune(args: &DaemonPruneArgs) -> Result<()> {
    match &args.command {
        DaemonPruneCommands::Plan(args) => handle_daemon_prune_plan(args),
        DaemonPruneCommands::Execute(args) => handle_daemon_prune_execute(args),
    }
}

#[cfg(unix)]
fn handle_daemon_prune_execute(args: &DaemonPruneExecuteArgs) -> Result<()> {
    use crate::control_plane::{IpcOutcome, IpcPayload, IpcResult};

    let response = super::send_singleton_request(IpcPayload::ExecutePostgresPrune {
        project_id: args.project_id.clone(),
        service_id: args.service_id.clone(),
        recovery_point_id: args.recovery_point_id.clone(),
        confirmation_token: args.confirmation_token.clone(),
    })?;
    let operation_id = match response.outcome() {
        IpcOutcome::Success {
            result: IpcResult::Accepted { operation_id },
        } => operation_id.clone(),
        IpcOutcome::Success { .. } => bail!("daemon returned an unexpected prune response"),
        IpcOutcome::Failure { diagnostics } => bail!(
            "Logical prune was rejected: {}",
            diagnostics
                .iter()
                .map(|item| format!("{}: {}", item.code(), item.message()))
                .collect::<Vec<_>>()
                .join("; ")
        ),
    };

    follow_prune(&operation_id)
}

#[cfg(unix)]
fn follow_prune(operation_id: &str) -> Result<()> {
    use crate::control_plane::{IpcEventKind, IpcOutcome, IpcPayload, IpcResult};

    let deadline = Instant::now() + PRUNE_TIMEOUT;
    let mut cursor = None;
    loop {
        if Instant::now() >= deadline {
            bail!("timed out waiting for logical prune '{operation_id}'");
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
            IpcOutcome::Failure { diagnostics } => bail!(
                "Logical prune event stream failed: {}",
                diagnostics
                    .iter()
                    .map(|item| format!("{}: {}", item.code(), item.message()))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        };
        cursor = Some(latest_sequence);
        for event in events {
            if event.operation_id() != operation_id {
                continue;
            }
            match event.kind() {
                IpcEventKind::Accepted => {}
                IpcEventKind::Completed => {
                    output::event(
                        "daemon",
                        LogLevel::Success,
                        "Logical resource pruned; verified recovery evidence retained",
                        Persistence::Persistent,
                    );

                    return Ok(());
                }
                IpcEventKind::Failed { code, message } => {
                    bail!("Logical prune failed ({code}): {message}")
                }
                IpcEventKind::Cancelled => bail!("Logical prune was cancelled"),
                IpcEventKind::Output { .. } => {
                    bail!("Logical prune returned unexpected output")
                }
            }
        }
        std::thread::sleep(EVENT_POLL_INTERVAL);
    }
}

#[cfg(not(unix))]
fn handle_daemon_prune_execute(_args: &DaemonPruneExecuteArgs) -> Result<()> {
    anyhow::bail!("Stackctl v8 requires a Unix host")
}

#[cfg(unix)]
fn handle_daemon_prune_plan(args: &DaemonPrunePlanArgs) -> Result<()> {
    use crate::control_plane::{IpcOutcome, IpcPayload, IpcResult};

    let response = super::send_singleton_request(IpcPayload::PlanPostgresPrune {
        project_id: args.project_id.clone(),
        service_id: args.service_id.clone(),
        recovery_point_id: args.recovery_point_id.clone(),
    })?;
    match response.outcome() {
        IpcOutcome::Success {
            result: IpcResult::PostgresPrunePlan { plan },
        } => {
            output::event(
                "daemon",
                LogLevel::Info,
                &format!(
                    "Logical prune plan: strategy={}, project={}, service={}, logical_resource={}, \
                     shared_resource={}, compatibility={}, credential={}, recovery_point={}, \
                     confirmation_token={}",
                    plan.strategy().as_str(),
                    plan.project_id(),
                    plan.service_id(),
                    plan.logical_resource_id(),
                    plan.shared_resource_id(),
                    plan.compatibility_fingerprint(),
                    plan.credential_id(),
                    plan.recovery_point_id(),
                    plan.confirmation_token(),
                ),
                Persistence::Persistent,
            );

            Ok(())
        }
        IpcOutcome::Success { .. } => bail!("daemon returned an unexpected prune-plan response"),
        IpcOutcome::Failure { diagnostics } => bail!(
            "Logical prune planning failed: {}",
            diagnostics
                .iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.code(), diagnostic.message()))
                .collect::<Vec<_>>()
                .join("; ")
        ),
    }
}

#[cfg(not(unix))]
fn handle_daemon_prune_plan(_args: &DaemonPrunePlanArgs) -> Result<()> {
    anyhow::bail!("Stackctl v8 requires a Unix host")
}
