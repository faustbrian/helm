//! cli handlers down cmd module.
//!
//! Contains cli handlers down cmd logic used by Helm command workflows.

use anyhow::Result;
use std::path::Path;

use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, docker, serve, swarm};

/// Handles the `down` CLI command.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_down(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    include_project_deps: bool,
    force: bool,
    parallel: usize,
    quiet: bool,
    no_color: bool,
    dry_run: bool,
    runtime_env: Option<&str>,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<()> {
    let workspace_root = config::project_root_with(config_path, project_root)?;
    cli::support::for_each_service(config, service, kind, None, parallel, |svc| {
        if !quiet {
            output::event(
                &svc.name,
                LogLevel::Info,
                "Stopping service",
                Persistence::Persistent,
            );
        }
        if svc.kind == config::Kind::App {
            serve::down(svc)
        } else {
            docker::down(svc)
        }
    })?;
    if include_project_deps {
        swarm::run_project_swarm_dependencies(
            "down",
            &workspace_root,
            quiet,
            no_color,
            dry_run,
            runtime_env,
            force,
        )?;
    }
    Ok(())
}
