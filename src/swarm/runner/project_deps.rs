//! Project-level swarm dependency execution.
//!
//! This module resolves the workspace context for the current project and runs
//! declared dependency targets before/after primary lifecycle commands.

use anyhow::Result;
use std::path::Path;

use super::super::injection::resolve_workspace_swarm_context;
use crate::cli::args::PortStrategyArg;
use crate::output::{self, LogLevel, Persistence};

/// Runs dependency targets declared by the current project's swarm context.
///
/// The dependency run is intentionally serialized (`parallel=1`, `fail_fast=true`)
/// to keep dependency ordering predictable and diagnostics easy to follow.
pub(super) fn run_project_swarm_dependencies(
    operation: &str,
    project_root: &Path,
    quiet: bool,
    no_color: bool,
    dry_run: bool,
    runtime_env: Option<&str>,
    force_down_deps: bool,
) -> Result<()> {
    let Some(context) = resolve_workspace_swarm_context(project_root)? else {
        anyhow::bail!(
            "could not resolve workspace swarm config for project root {}",
            project_root.display()
        );
    };

    if context.target.depends_on.is_empty() {
        if !quiet {
            output::event(
                "swarm",
                LogLevel::Info,
                &format!(
                    "No workspace dependencies configured for project '{}'",
                    context.target.name
                ),
                Persistence::Persistent,
            );
        }
        return Ok(());
    }

    if !quiet {
        output::event(
            "swarm",
            LogLevel::Info,
            &format!(
                "Running project dependencies for '{}' from workspace {}",
                context.target.name,
                context.workspace_root.display()
            ),
            Persistence::Persistent,
        );
    }

    let mut workspace_config = context.workspace_config.clone();
    if let Some(runtime_env) = runtime_env {
        crate::config::apply_runtime_env(&mut workspace_config, runtime_env)?;
    }

    let command = vec![operation.to_owned()];
    super::run_swarm(
        &workspace_config,
        &command,
        &context.target.depends_on,
        true,
        force_down_deps,
        1,
        true,
        PortStrategyArg::Random,
        None,
        false,
        quiet,
        no_color,
        dry_run,
        false,
        runtime_env,
        None,
        Some(context.workspace_root.as_path()),
    )
}
