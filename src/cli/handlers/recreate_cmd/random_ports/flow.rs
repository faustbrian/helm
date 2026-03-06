//! recreate random-ports execution and persistence flow.

use anyhow::Result;
use std::path::Path;

use crate::{cli, config};

use super::runtime::RecreateRuntimeContext;

pub(super) fn run_recreate_flow<F>(
    planned: &[config::ServiceConfig],
    config: &mut config::Config,
    recreate_context: RecreateRuntimeContext<'_>,
    env_path: Option<&Path>,
    save_ports: bool,
    quiet: bool,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
    parallel: usize,
    runtime_runner: F,
) -> Result<()>
where
    F: Fn(&config::ServiceConfig, &RecreateRuntimeContext<'_>) -> Result<()> + Sync,
{
    cli::support::run_random_ports_flow(
        planned,
        parallel,
        |runtime| runtime.kind == config::Kind::App,
        |runtime| runtime_runner(runtime, &recreate_context),
        |runtime| runtime,
        config,
        cli::support::RandomPortsPersistenceOptions {
            env_path,
            save_ports,
            command: "recreate",
            quiet,
            config_path,
            project_root,
        },
    )?;
    Ok(())
}
