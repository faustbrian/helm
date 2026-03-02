//! cli support for persisting runtime port changes.
//!
//! Centralizes env/config updates that occur after services are started with
//! randomized or reassigned ports.

use anyhow::Result;
use std::path::Path;

use crate::{
    config, env,
    output::{self, LogLevel, Persistence},
};

/// Persists runtime port changes to env and config files.
pub(crate) fn persist_random_runtime_ports<'a, I>(
    services: I,
    config_data: &mut config::Config,
    env_path: Option<&Path>,
    save_ports: bool,
    command: &str,
    quiet: bool,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<Option<std::path::PathBuf>>
where
    I: IntoIterator<Item = &'a config::ServiceConfig>,
{
    for service in services {
        if let Some(path) = env_path {
            env::update_env(service, path, true)?;
        }
        if save_ports {
            config::update_service_port(config_data, &service.name, service.port)?;
        }
    }

    if !save_ports {
        return Ok(None);
    }

    let path = config::save_config_with(
        config_data,
        config::SaveConfigPathOptions::new(config_path, project_root),
    )?;
    if !quiet {
        output::event(
            command,
            LogLevel::Success,
            &format!("Persisted random ports to {}", path.display()),
            Persistence::Persistent,
        );
    }
    Ok(Some(path))
}
