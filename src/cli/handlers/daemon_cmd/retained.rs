use crate::cli::args::DaemonRetainedArgs;
use crate::control_plane::{IpcOutcome, IpcPayload, IpcProjectStatus, IpcResult};
use anyhow::{Result, bail};
use std::io::{Write, stdout};

/// Renders retained state without requiring a deleted project's former path.
pub(super) fn handle_daemon_retained(args: &DaemonRetainedArgs) -> Result<()> {
    if args.format != "table" && args.format != "json" {
        bail!("retained status format must be 'table' or 'json'");
    }
    let response = super::send_singleton_request(IpcPayload::RetainedProjectStatus)?;
    let IpcOutcome::Success {
        result: IpcResult::RetainedProjectStatus { projects },
    } = response.outcome()
    else {
        bail!("retained project status failed: {:?}", response.outcome());
    };

    if args.format == "json" {
        serde_json::to_writer_pretty(stdout(), projects)?;
        println!();
        return Ok(());
    }
    render_table(&mut stdout(), projects)
}

fn render_table(writer: &mut impl Write, projects: &[IpcProjectStatus]) -> Result<()> {
    writeln!(
        writer,
        "PROJECT\tSERVICE\tKIND\tSCOPE\tDATA_SCOPE\tLIFECYCLE"
    )?;
    for project in projects {
        for resource in project.resources() {
            writeln!(
                writer,
                "{}\t{}\t{}\t{}\t{}\t{}",
                project.project(),
                resource.service(),
                resource.kind(),
                if resource.shared() {
                    "shared"
                } else {
                    "project"
                },
                resource.data_lifecycle().as_str(),
                resource.lifecycle().as_str(),
            )?;
        }
    }
    Ok(())
}
