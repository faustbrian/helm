//! cli handlers recreate cmd module.
//!
//! Contains cli handlers recreate cmd logic used by Helm command workflows.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, docker, env, serve};

mod random_ports;

/// Handles the `recreate` CLI command.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_recreate(
    config: &mut config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    healthy: bool,
    timeout: u64,
    random_ports: bool,
    save_ports: bool,
    env_output: bool,
    parallel: usize,
    quiet: bool,
    runtime_env: Option<&str>,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
    config_path_buf: &Option<PathBuf>,
    project_root_buf: &Option<PathBuf>,
) -> Result<()> {
    let workspace_root = config::project_root_with(config_path, project_root)?;

    if random_ports {
        return random_ports::handle_random_ports_recreate(
            config,
            service,
            kind,
            healthy,
            timeout,
            save_ports,
            env_output,
            quiet,
            runtime_env,
            config_path,
            project_root,
            config_path_buf,
            project_root_buf,
            parallel,
            &workspace_root,
        );
    }

    let inferred_env = env::inferred_app_env(config);
    cli::support::for_each_service(config, service, kind, None, parallel, |svc| {
        if !quiet {
            output::event(
                &svc.name,
                LogLevel::Info,
                "Recreating service",
                Persistence::Persistent,
            );
        }
        recreate_service(svc, healthy, timeout, &workspace_root, &inferred_env)
    })?;

    Ok(())
}

pub(super) fn recreate_service(
    service: &config::ServiceConfig,
    healthy: bool,
    timeout: u64,
    workspace_root: &Path,
    inferred_env: &std::collections::HashMap<String, String>,
) -> Result<()> {
    if service.kind == config::Kind::App {
        serve::run(
            service,
            true,
            service.trust_container_ca,
            true,
            workspace_root,
            inferred_env,
            true,
        )?;
        if healthy {
            serve::wait_until_http_healthy(service, timeout, 2, None)?;
        }
    } else {
        docker::recreate(service)?;
        if healthy {
            docker::wait_until_healthy(service, timeout, 2, None)?;
        }
    }

    Ok(())
}
