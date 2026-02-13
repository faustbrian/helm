//! Planning phase for swarm execution.
//!
//! These helpers validate CLI intent and turn config + workspace context into the
//! exact ordered target list to execute.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::swarm::targets::{
    ResolvedSwarmTarget, bootstrap_swarm_targets, enforce_shared_down_dependency_guard,
    ensure_swarm_target_configs_exist, resolve_swarm_targets,
};

pub(super) fn validate_swarm_invocation<'a>(
    command: &'a [String],
    parallel: usize,
) -> Result<&'a str> {
    if parallel == 0 {
        anyhow::bail!("--parallel must be >= 1");
    }

    let Some(subcommand) = command.first() else {
        anyhow::bail!("missing swarm command");
    };
    if subcommand == "swarm" {
        anyhow::bail!("nested `helm swarm` is not supported");
    }

    Ok(subcommand)
}

/// Resolves the workspace root used for swarm target discovery.
pub(super) fn resolve_workspace_root(
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<PathBuf> {
    if config_path.is_none() && project_root.is_none() {
        return crate::config::project_root();
    }
    crate::config::project_root_with(config_path, project_root)
}

/// Handles `swarm ls` shortcut mode.
///
/// Returns `Ok(true)` when listing was emitted and no execution should occur.
pub(super) fn handle_ls_command(
    config: &crate::config::Config,
    workspace_root: &Path,
    only: &[String],
    subcommand: &str,
    command_len: usize,
) -> Result<bool> {
    if subcommand == "ls" && command_len == 1 {
        for target in resolve_swarm_targets(config, workspace_root, only, false)? {
            println!("{}\t{}", target.name, target.root.display());
        }
        return Ok(true);
    }

    Ok(false)
}

/// Resolves and orders the final execution target list for a swarm run.
///
/// Important ordering rule:
/// - `swarm down` with dependencies reverses target order so dependents are
///   stopped before the services they rely on.
pub(super) fn resolve_execution_targets(
    config: &crate::config::Config,
    workspace_root: &Path,
    command: &[String],
    only: &[String],
    include_deps: bool,
    force_down_deps: bool,
    subcommand: &str,
    quiet: bool,
) -> Result<Vec<ResolvedSwarmTarget>> {
    if command.first().is_some_and(|sub| sub == "logs") && command.iter().any(|arg| arg == "--tui")
    {
        anyhow::bail!("`helm swarm logs --tui` was removed. Use `helm swarm logs` instead.");
    }

    let mut targets = resolve_swarm_targets(config, workspace_root, only, include_deps)?;
    if subcommand == "up" {
        bootstrap_swarm_targets(config, &targets, quiet)?;
    }
    ensure_swarm_target_configs_exist(&targets)?;

    if include_deps && subcommand == "down" {
        enforce_shared_down_dependency_guard(
            config,
            only,
            &targets,
            force_down_deps,
            workspace_root,
        )?;
        targets.reverse();
    }

    Ok(targets)
}
