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

const fn default_random_ports_enabled() -> bool {
    true
}

const fn default_healthy_enabled() -> bool {
    true
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_up(
    config: &mut config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    profile: Option<&str>,
    healthy: bool,
    timeout: u64,
    pull_policy: docker::PullPolicy,
    recreate: bool,
    random_ports: bool,
    force_random_ports: bool,
    port_strategy: PortStrategyArg,
    port_seed: Option<&str>,
    persist_ports: bool,
    write_env: bool,
    with_project_deps: bool,
    with_data: bool,
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
        if write_env {
            anyhow::bail!("--repro cannot be combined with --write-env");
        }
        if persist_ports {
            anyhow::bail!("--repro cannot persist runtime-discovered ports");
        }
        config::verify_lockfile_with(config, config_path, project_root)?;
    }

    let workspace_root = config::project_root_with(config_path, project_root)?;
    if with_project_deps {
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

    let use_healthy = healthy || default_healthy_enabled();
    let use_random_ports = random_ports || default_random_ports_enabled();
    if use_random_ports {
        run_random_ports_up(
            config,
            service,
            kind,
            profile,
            use_healthy,
            timeout,
            pull_policy,
            recreate,
            force_random_ports,
            port_strategy,
            port_seed,
            persist_ports,
            write_env,
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
            use_healthy,
            timeout,
            pull_policy,
            recreate,
            quiet,
            &workspace_root,
            &project_dependency_env,
        )?;
    }

    if with_data {
        apply_data_seeds(config, service, kind, profile, &workspace_root, quiet)?;
    }

    Ok(())
}
