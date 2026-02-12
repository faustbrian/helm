use anyhow::Result;
use std::path::Path;

use super::{args, project_deps, run};
use crate::cli::args::PortStrategyArg;

pub(crate) use run::run_swarm;

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn swarm_child_args(
    target: &super::super::targets::ResolvedSwarmTarget,
    command: &[String],
    port_strategy: PortStrategyArg,
    port_seed: Option<&str>,
    env_output: bool,
    quiet: bool,
    no_color: bool,
    dry_run: bool,
    repro: bool,
    runtime_env: Option<&str>,
) -> Vec<String> {
    args::swarm_child_args(
        target,
        command,
        port_strategy,
        port_seed,
        env_output,
        quiet,
        no_color,
        dry_run,
        repro,
        runtime_env,
    )
}

pub(crate) fn run_project_swarm_dependencies(
    operation: &str,
    project_root: &Path,
    quiet: bool,
    no_color: bool,
    dry_run: bool,
    runtime_env: Option<&str>,
    force_down_deps: bool,
) -> Result<()> {
    project_deps::run_project_swarm_dependencies(
        operation,
        project_root,
        quiet,
        no_color,
        dry_run,
        runtime_env,
        force_down_deps,
    )
}
