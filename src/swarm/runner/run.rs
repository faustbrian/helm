//! End-to-end swarm command orchestration.
//!
//! This module validates invocation, resolves targets, runs each target command,
//! and returns a consolidated success/failure result.

use anyhow::{Context, Result};
use std::path::Path;

use crate::cli::args::PortStrategyArg;
use crate::output::{self, LogLevel, Persistence};

use super::super::target_exec::{OutputMode, run_swarm_target};
use super::{
    execution,
    planning::{
        handle_ls_command, resolve_execution_targets, resolve_workspace_root,
        validate_swarm_invocation,
    },
    summary,
};

/// Executes a swarm command across resolved targets.
///
/// Behavior notes:
/// - `swarm ls` is handled as a read-only listing shortcut.
/// - `swarm ps` uses passthrough output to preserve tabular formatting.
/// - Other subcommands use structured logging output.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_swarm(
    config: &crate::config::Config,
    command: &[String],
    only: &[String],
    include_deps: bool,
    force_down_deps: bool,
    parallel: usize,
    fail_fast: bool,
    port_strategy: PortStrategyArg,
    port_seed: Option<&str>,
    env_output: bool,
    quiet: bool,
    no_color: bool,
    dry_run: bool,
    repro: bool,
    runtime_env: Option<&str>,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<()> {
    let subcommand = validate_swarm_invocation(command, parallel)?;
    let workspace_root = resolve_workspace_root(config_path, project_root)?;

    if handle_ls_command(config, &workspace_root, only, subcommand, command.len())? {
        return Ok(());
    }

    let targets = resolve_execution_targets(
        config,
        &workspace_root,
        command,
        only,
        include_deps,
        force_down_deps,
        subcommand,
        quiet,
    )?;

    let helm_executable =
        std::env::current_exe().context("failed to resolve current executable")?;
    let output_mode = if subcommand == "ps" {
        OutputMode::Passthrough
    } else {
        OutputMode::Logged
    };

    if !quiet {
        match output_mode {
            OutputMode::Logged => output::event(
                "swarm",
                LogLevel::Info,
                &format!(
                    "Running `helm {}` across {} target(s)",
                    command.join(" "),
                    targets.len()
                ),
                Persistence::Persistent,
            ),
            OutputMode::Passthrough => {
                println!(
                    "Running `helm {}` across {} target(s)",
                    command.join(" "),
                    targets.len()
                );
            }
        }
    }

    let (results, cancelled) = execution::run_targets(
        &targets,
        command,
        output_mode,
        parallel,
        fail_fast,
        port_strategy,
        port_seed,
        env_output,
        quiet,
        no_color,
        dry_run,
        repro,
        runtime_env,
        &helm_executable,
        run_swarm_target,
    )?;

    summary::summarize_results(&results, cancelled, output_mode)
}
