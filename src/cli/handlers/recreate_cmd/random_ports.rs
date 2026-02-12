use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, env};

use super::recreate_service;

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_random_ports_recreate(
    config: &mut config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    healthy: bool,
    timeout: u64,
    persist_ports: bool,
    write_env: bool,
    quiet: bool,
    runtime_env: Option<&str>,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
    config_path_buf: &Option<PathBuf>,
    project_root_buf: &Option<PathBuf>,
    parallel: usize,
    workspace_root: &Path,
    inferred_env: &HashMap<String, String>,
) -> Result<()> {
    if parallel > 1 {
        anyhow::bail!("--random-ports cannot be combined with --parallel > 1");
    }

    let env_path = if write_env {
        Some(cli::support::default_env_path(
            config_path_buf,
            project_root_buf,
            &None,
            runtime_env,
        )?)
    } else {
        None
    };
    let selected: Vec<config::ServiceConfig> =
        cli::support::selected_services(config, service, kind, None)?
            .into_iter()
            .cloned()
            .collect();

    for mut runtime in selected {
        runtime.port = cli::support::random_free_port(&runtime.host)?;
        if runtime.driver == config::Driver::Mailhog && runtime.smtp_port.is_some() {
            runtime.smtp_port = Some(random_distinct_port(&runtime.host, runtime.port)?);
        }
        if !quiet {
            output::event(
                &runtime.name,
                LogLevel::Info,
                &format!("Recreating service on random port {}", runtime.port),
                Persistence::Persistent,
            );
        }

        recreate_service(&runtime, healthy, timeout, workspace_root, inferred_env)?;
        if let Some(path) = env_path.as_deref() {
            env::update_env(&runtime, path, true)?;
        }

        if persist_ports {
            config::update_service_port(config, &runtime.name, runtime.port)?;
        }
    }

    if persist_ports {
        let path = config::save_config_with(config, config_path, project_root)?;
        if !quiet {
            output::event(
                "recreate",
                LogLevel::Success,
                &format!("Persisted random ports to {}", path.display()),
                Persistence::Persistent,
            );
        }
    }

    Ok(())
}

fn random_distinct_port(host: &str, avoid: u16) -> Result<u16> {
    for _ in 0..20 {
        let candidate = cli::support::random_free_port(host)?;
        if candidate != avoid {
            return Ok(candidate);
        }
    }
    anyhow::bail!("failed to allocate random port distinct from {}", avoid);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_distinct_port_avoids_requested_port() {
        let avoid = cli::support::random_free_port("127.0.0.1").expect("allocate avoid port");
        let candidate = random_distinct_port("127.0.0.1", avoid).expect("allocate candidate port");
        assert_ne!(candidate, avoid);
    }
}
