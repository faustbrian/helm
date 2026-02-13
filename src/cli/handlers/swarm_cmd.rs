//! cli handlers swarm cmd module.
//!
//! Contains cli handlers swarm cmd logic used by Helm command workflows.

use anyhow::Result;
use std::path::Path;

use crate::cli::args::PortStrategyArg;
use crate::{config, swarm};

/// Handles the `swarm` CLI command.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_swarm(
    config: &config::Config,
    command: &[String],
    only: &[String],
    no_deps: bool,
    force: bool,
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
    if repro && command.first().is_some_and(|subcommand| subcommand == "up") {
        if env_output {
            anyhow::bail!("--repro cannot be combined with --env-output");
        }
        config::verify_lockfile_with(config, config_path, project_root)?;
    }

    swarm::run_swarm(
        config,
        command,
        only,
        !no_deps,
        force,
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
        config_path,
        project_root,
    )
}
