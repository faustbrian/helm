//! Secret-free strict v8 project status from authoritative daemon state.

use std::io::{Write, stdout};

use anyhow::{Result, bail};

use crate::cli::args::{Cli, Commands};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::control_plane::IpcProjectStatus;

use super::v8_project::resolve_v8_project;
use super::v8_project_status::request_v8_project_status;

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

    let status = request_v8_project_status(project.root())?;
    if args.format == "json" {
        serde_json::to_writer_pretty(stdout(), &status)?;
        writeln!(stdout())?;
    } else {
        render_table(&mut stdout(), &status)?;
    }

    Ok(true)
}

fn render_table(writer: &mut impl Write, status: &IpcProjectStatus) -> Result<()> {
    writeln!(writer, "PROJECT\t{}", status.project())?;
    for route in status.routes() {
        writeln!(writer, "ROUTE\t{route}")?;
    }
    writeln!(
        writer,
        "SERVICE\tKIND\tSCOPE\tDATA_SCOPE\tLIFECYCLE\tHEALTH\tOBSERVED_AT"
    )?;
    for resource in status.resources() {
        let scope = if resource.shared() {
            "shared"
        } else {
            "project"
        };
        writeln!(
            writer,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            resource.service(),
            resource.kind(),
            scope,
            resource.data_lifecycle().as_str(),
            resource.lifecycle().as_str(),
            resource.health().as_str(),
            resource
                .observed_at_unix_seconds()
                .map_or_else(|| "-".to_owned(), |value| value.to_string())
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::render_table;
    use crate::control_plane::{
        IpcDataLifecycle, IpcProjectStatus, IpcResourceHealth, IpcResourceLifecycle,
        IpcResourceStatus,
    };

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
                    IpcResourceHealth::Healthy,
                    Some(10_000),
                    false,
                ),
                IpcResourceStatus::with_data_lifecycle(
                    "db".to_owned(),
                    "postgresql".to_owned(),
                    IpcResourceLifecycle::Active,
                    IpcResourceHealth::Unknown,
                    None,
                    true,
                    IpcDataLifecycle::LogicalResource,
                ),
            ],
        );
        let mut output = Vec::new();

        render_table(&mut output, &status).expect("render status");

        assert_eq!(
            String::from_utf8(output).expect("utf8 status"),
            "PROJECT\tbill\n\
             ROUTE\tbill-app.stackctl.localhost\n\
             SERVICE\tKIND\tSCOPE\tDATA_SCOPE\tLIFECYCLE\tHEALTH\tOBSERVED_AT\n\
             app\tproject_application\tproject\tnone\tactive\thealthy\t10000\n\
             db\tpostgresql\tshared\tlogical_resource\tactive\tunknown\t-\n"
        );
    }
}
