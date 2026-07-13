//! Secret-free strict v8 project status from authoritative daemon state.

use std::io::{Write, stdout};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Result, anyhow, bail};

use crate::cli::args::{Cli, Commands};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::control_plane::{
    IpcDiagnostic, IpcOutcome, IpcPayload, IpcProjectStatus, IpcRequest, IpcResponse, IpcResult,
    default_unix_daemon_runtime_directory, send_unix_request,
};

use super::v8_project::resolve_v8_project;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
static STATUS_REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(crate) fn handle_v8_status(cli: &Cli, context: &CliDispatchContext<'_>) -> Result<bool> {
    let Commands::Ps(args) = &cli.command else {
        return Ok(false);
    };
    let Some(project) = resolve_v8_project(context)? else {
        return Ok(false);
    };
    if args.kind().is_some() || args.driver().is_some() {
        bail!("v8 status reports exact declared services; --kind and --driver are not supported");
    }
    if !matches!(args.format.as_str(), "table" | "json") {
        bail!("v8 status format must be 'table' or 'json'");
    }

    let request_id = format!(
        "project-status-{}-{}",
        std::process::id(),
        STATUS_REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let socket_path = default_unix_daemon_runtime_directory()?.join("daemon.sock");
    let response = send_unix_request(
        &socket_path,
        &IpcRequest::new(
            request_id,
            IpcPayload::ProjectStatus {
                canonical_path: project.root().to_path_buf(),
            },
        ),
        REQUEST_TIMEOUT,
    )?;
    let status = project_status(&response)?;
    if args.format == "json" {
        serde_json::to_writer_pretty(stdout(), status)?;
        writeln!(stdout())?;
    } else {
        render_table(&mut stdout(), status)?;
    }

    Ok(true)
}

fn project_status(response: &IpcResponse) -> Result<&IpcProjectStatus> {
    match response.outcome() {
        IpcOutcome::Success {
            result: IpcResult::ProjectStatus { project },
        } => Ok(project),
        IpcOutcome::Success { .. } => bail!("daemon returned an unexpected status response"),
        IpcOutcome::Failure { diagnostics } => Err(diagnostic_error(diagnostics)),
    }
}

fn diagnostic_error(diagnostics: &[IpcDiagnostic]) -> anyhow::Error {
    let message = diagnostics
        .iter()
        .map(|diagnostic| format!("{}: {}", diagnostic.code(), diagnostic.message()))
        .collect::<Vec<_>>()
        .join("; ");
    anyhow!("daemon request failed: {message}")
}

fn render_table(writer: &mut impl Write, status: &IpcProjectStatus) -> Result<()> {
    writeln!(writer, "PROJECT\t{}", status.project())?;
    for route in status.routes() {
        writeln!(writer, "ROUTE\t{route}")?;
    }
    writeln!(writer, "SERVICE\tKIND\tSCOPE\tLIFECYCLE")?;
    for resource in status.resources() {
        let scope = if resource.shared() {
            "shared"
        } else {
            "project"
        };
        writeln!(
            writer,
            "{}\t{}\t{}\t{}",
            resource.service(),
            resource.kind(),
            scope,
            resource.lifecycle().as_str()
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::render_table;
    use crate::control_plane::{IpcProjectStatus, IpcResourceLifecycle, IpcResourceStatus};

    #[test]
    fn table_status_distinguishes_project_and_shared_resources() {
        let status = IpcProjectStatus::new(
            "bill".to_owned(),
            vec!["bill-app.stackctl.localhost".to_owned()],
            vec![
                IpcResourceStatus::new(
                    "app".to_owned(),
                    "project_application".to_owned(),
                    IpcResourceLifecycle::Active,
                    false,
                ),
                IpcResourceStatus::new(
                    "db".to_owned(),
                    "postgresql".to_owned(),
                    IpcResourceLifecycle::Active,
                    true,
                ),
            ],
        );
        let mut output = Vec::new();

        render_table(&mut output, &status).expect("render status");

        assert_eq!(
            String::from_utf8(output).expect("utf8 status"),
            "PROJECT\tbill\n\
             ROUTE\tbill-app.stackctl.localhost\n\
             SERVICE\tKIND\tSCOPE\tLIFECYCLE\n\
             app\tproject_application\tproject\tactive\n\
             db\tpostgresql\tshared\tactive\n"
        );
    }
}
