use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::cli::args::PortStrategyArg;
use crate::{config, docker, swarm};

mod data_seed;
mod random_ports;
mod startup;

use data_seed::apply_data_seeds;
use random_ports::run_random_ports_up;
use startup::run_standard_up;

const fn default_publish_all_enabled() -> bool {
    true
}

const fn default_wait_enabled() -> bool {
    true
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_up(
    config: &mut config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    profile: Option<&str>,
    wait: bool,
    wait_timeout: u64,
    pull_policy: docker::PullPolicy,
    force_recreate: bool,
    publish_all: bool,
    port_strategy: PortStrategyArg,
    port_seed: Option<&str>,
    save_ports: bool,
    env_output: bool,
    include_project_deps: bool,
    seed: bool,
    parallel: usize,
    quiet: bool,
    no_color: bool,
    dry_run: bool,
    repro: bool,
    runtime_env: Option<&str>,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
    config_path_buf: &Option<PathBuf>,
    project_root_buf: &Option<PathBuf>,
) -> Result<()> {
    if repro {
        if env_output {
            anyhow::bail!("--repro cannot be combined with --env-output");
        }
        if save_ports {
            anyhow::bail!("--repro cannot persist runtime-discovered ports");
        }
        config::verify_lockfile_with(config, config_path, project_root)?;
    }

    let workspace_root = config::project_root_with(config_path, project_root)?;
    if include_project_deps {
        swarm::run_project_swarm_dependencies(
            "up",
            &workspace_root,
            quiet,
            no_color,
            dry_run,
            runtime_env,
            false,
        )?;
    }
    let project_dependency_env = swarm::resolve_project_dependency_injected_env(&workspace_root)?;

    let use_wait = wait || default_wait_enabled();
    let use_publish_all = publish_all || default_publish_all_enabled();
    if use_publish_all {
        run_random_ports_up(
            config,
            service,
            kind,
            profile,
            use_wait,
            wait_timeout,
            pull_policy,
            force_recreate,
            true,
            port_strategy,
            port_seed,
            save_ports,
            env_output,
            parallel,
            quiet,
            runtime_env,
            &workspace_root,
            &project_dependency_env,
            config_path,
            project_root,
            config_path_buf,
            project_root_buf,
        )?;
    } else {
        run_standard_up(
            config,
            service,
            kind,
            profile,
            use_wait,
            wait_timeout,
            pull_policy,
            force_recreate,
            quiet,
            &workspace_root,
            &project_dependency_env,
        )?;
    }

    if seed {
        apply_data_seeds(config, service, kind, profile, &workspace_root, quiet)?;
    }

    Ok(())
}
