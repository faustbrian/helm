//! Explicit ordered workflow execution through singleton-daemon operations.

use anyhow::{Context, Result, bail};
use std::path::Path;
use std::time::Duration;

use crate::cli::args::{Cli, Commands};
use crate::cli::browser_opener::try_open_in_browser;
use crate::cli::dispatch::context::CliDispatchContext;
use crate::control_plane::{
    IpcPayload, IpcProjectCommand, IpcRequest, RawWorkflowStep,
    default_unix_daemon_runtime_directory, send_unix_request,
};

use super::v8_open_cmd::ensure_routes_ready;
use super::v8_project::{V8Project, resolve_v8_project};
use super::v8_project_command::{
    accepted_operation_id, execute_project_command, follow_operation, next_request_id,
};
use super::v8_project_status::request_v8_project_status;
use super::v8_url_cmd::{render_routes, select_routes};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) fn handle_v8_workflow(cli: &Cli, context: &CliDispatchContext<'_>) -> Result<bool> {
    let Commands::Run(args) = &cli.command else {
        return Ok(false);
    };
    let Some(project) = resolve_v8_project(context)? else {
        return Ok(false);
    };
    if context.dry_run() {
        bail!("--dry-run is not supported for destructive v8 workflows");
    }
    let workflow = project
        .config()
        .workflows()
        .get(&args.workflow)
        .with_context(|| {
            format!(
                "workflow '{}' is not declared in .stackctl.yaml",
                args.workflow
            )
        })?;
    if workflow.steps().is_empty() {
        bail!("workflow '{}' has no steps", args.workflow);
    }

    for step in workflow.steps() {
        execute_step(&project, step, context)?;
    }

    Ok(true)
}

fn execute_step(
    project: &V8Project,
    step: &RawWorkflowStep,
    context: &CliDispatchContext<'_>,
) -> Result<()> {
    if let Some(file) = step.file() {
        restore_database_dump(project, step, file)?;
        if let (Some(service), Some(connection)) =
            (step.migration_service(), step.migration_connection())
        {
            execute_project_command(
                project.root().to_path_buf(),
                service.to_owned(),
                IpcProjectCommand::Artisan {
                    arguments: vec!["migrate".to_owned(), format!("--database={connection}")],
                    browser: false,
                },
            )?;
        }

        return Ok(());
    }

    open_service(project, step.service(), context)
}

fn restore_database_dump(
    project: &V8Project,
    step: &RawWorkflowStep,
    relative_file: &Path,
) -> Result<()> {
    let file = project
        .root()
        .join(relative_file)
        .canonicalize()
        .with_context(|| format!("database dump '{}' is unavailable", relative_file.display()))?;
    if !file.starts_with(project.root()) {
        bail!("database dump must remain inside the project directory");
    }
    let socket_path = default_unix_daemon_runtime_directory()?.join("daemon.sock");
    let operation_id = next_request_id("database-dump-restore");
    let response = send_unix_request(
        &socket_path,
        &IpcRequest::new(
            operation_id.clone(),
            IpcPayload::RestoreProjectDatabaseDump {
                canonical_path: project.root().to_path_buf(),
                service: step.service().to_owned(),
                file,
                archive_entry: step.archive_entry().map(str::to_owned),
                reset: step.reset(),
            },
        ),
        REQUEST_TIMEOUT,
    )?;
    let accepted = accepted_operation_id(&response)?;
    if accepted != operation_id {
        bail!("daemon accepted unexpected operation '{accepted}'");
    }

    follow_operation(&socket_path, &operation_id)
}

fn open_service(
    project: &V8Project,
    service: &str,
    context: &CliDispatchContext<'_>,
) -> Result<()> {
    let status = request_v8_project_status(project.root())?;
    let routes = select_routes(&status, &project.service_names(), Some(service))?;
    ensure_routes_ready(&status, &routes)?;
    if context.non_interactive() {
        render_routes(&mut std::io::stdout(), &routes, true, "table")?;
    } else {
        for (_, url) in routes {
            try_open_in_browser(&url)?;
        }
    }

    Ok(())
}
