use crate::cli::args::{DaemonPruneArgs, DaemonPruneCommands, DaemonPrunePlanArgs};
use crate::output::{self, LogLevel, Persistence};
use anyhow::{Result, bail};

pub(super) fn handle_daemon_prune(args: &DaemonPruneArgs) -> Result<()> {
    match &args.command {
        DaemonPruneCommands::Plan(args) => handle_daemon_prune_plan(args),
    }
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
                    "PostgreSQL prune plan: project={}, service={}, logical_resource={}, \
                     shared_resource={}, compatibility={}, credential={}, recovery_point={}, \
                     confirmation_token={}",
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
            "PostgreSQL prune planning failed: {}",
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
    anyhow::bail!("the v8 singleton daemon requires the Windows named-pipe runtime")
}
