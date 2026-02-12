use anyhow::Result;
use std::path::Path;

use crate::cli::args::PortStrategyArg;
use crate::{config, swarm};

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_swarm(
    config: &config::Config,
    command: &[String],
    only: &[String],
    without_deps: bool,
    force_down_deps: bool,
    parallel: usize,
    fail_fast: bool,
    force_random_ports: bool,
    port_strategy: PortStrategyArg,
    port_seed: Option<&str>,
    write_env: bool,
    quiet: bool,
    no_color: bool,
    dry_run: bool,
    repro: bool,
    runtime_env: Option<&str>,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<()> {
    if repro && command.first().is_some_and(|subcommand| subcommand == "up") {
        if write_env {
            anyhow::bail!("--repro cannot be combined with --write-env");
        }
        config::verify_lockfile_with(config, config_path, project_root)?;
    }

    swarm::run_swarm(
        config,
        command,
        only,
        !without_deps,
        force_down_deps,
        parallel,
        fail_fast,
        force_random_ports,
        port_strategy,
        port_seed,
        write_env,
        quiet,
        no_color,
        dry_run,
        repro,
        runtime_env,
        config_path,
        project_root,
    )
}
