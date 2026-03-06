//! Preflight preparation for `up` command runtime context.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use crate::{cli, config, swarm};

pub(super) struct PreparedUpContext {
    pub(super) workspace_root: PathBuf,
    pub(super) project_dependency_env: HashMap<String, String>,
    pub(super) env_path: Option<PathBuf>,
}

pub(super) struct PrepareUpContextOptions<'a> {
    pub(super) repro: bool,
    pub(super) env_output: bool,
    pub(super) save_ports: bool,
    pub(super) config_path: Option<&'a Path>,
    pub(super) project_root: Option<&'a Path>,
    pub(super) include_project_deps: bool,
    pub(super) quiet: bool,
    pub(super) no_color: bool,
    pub(super) dry_run: bool,
    pub(super) runtime_env: Option<&'a str>,
}

pub(super) fn prepare_up_context(
    config: &mut config::Config,
    options: PrepareUpContextOptions<'_>,
) -> Result<PreparedUpContext> {
    if options.repro {
        super::options::validate_repro_flags(options.env_output, options.save_ports)?;
        config::verify_lockfile_with(
            config,
            config::ProjectRootPathOptions::new(options.config_path, options.project_root),
        )?;
    }

    let workspace_root =
        cli::support::workspace_with_project_deps(cli::support::WorkspaceWithProjectDepsOptions {
            operation: "up",
            config_path: options.config_path,
            project_root: options.project_root,
            include_project_deps: options.include_project_deps,
            quiet: options.quiet,
            no_color: options.no_color,
            dry_run: options.dry_run,
            runtime_env: options.runtime_env,
            force_down_deps: false,
        })?;

    let project_dependency_env = swarm::resolve_project_dependency_injected_env(&workspace_root)?;
    let env_path = cli::support::env_output_path(
        options.env_output,
        options.config_path,
        options.project_root,
        options.runtime_env,
    )?;

    Ok(PreparedUpContext {
        workspace_root,
        project_dependency_env,
        env_path,
    })
}
