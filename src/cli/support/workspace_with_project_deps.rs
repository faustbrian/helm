//! Shared workspace resolution with optional project dependency execution.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::swarm;

pub(crate) struct WorkspaceWithProjectDepsOptions<'a> {
    pub(crate) operation: &'a str,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
    pub(crate) include_project_deps: bool,
    pub(crate) quiet: bool,
    pub(crate) no_color: bool,
    pub(crate) dry_run: bool,
    pub(crate) runtime_env: Option<&'a str>,
    pub(crate) force_down_deps: bool,
}

pub(crate) fn workspace_with_project_deps(
    options: WorkspaceWithProjectDepsOptions<'_>,
) -> Result<PathBuf> {
    let workspace_root = super::workspace_root(options.config_path, options.project_root)?;

    if options.include_project_deps {
        swarm::run_project_swarm_dependencies(swarm::RunProjectSwarmDependenciesOptions {
            operation: options.operation,
            project_root: &workspace_root,
            quiet: options.quiet,
            no_color: options.no_color,
            dry_run: options.dry_run,
            runtime_env: options.runtime_env,
            force_down_deps: options.force_down_deps,
        })?;
    }

    Ok(workspace_root)
}
