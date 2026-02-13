//! config api load save module.
//!
//! Contains config api load save logic used by Helm command workflows.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use super::super::{Config, RawConfig, expansion, paths, validation};

/// Loads the configuration file using default discovery.
///
/// # Errors
///
/// Returns an error if the config cannot be found, read, or parsed.
pub fn load_config() -> Result<Config> {
    load_config_with(None, None)
}

/// Loads configuration from explicit config path or project root override.
///
/// # Errors
///
/// Returns an error if path resolution, reading, or parsing fails.
pub fn load_config_with(config_path: Option<&Path>, project_root: Option<&Path>) -> Result<Config> {
    let config_path = paths::resolve_config_path(config_path, project_root)?;

    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read config file at {}", config_path.display()))?;

    let raw_config: RawConfig = toml::from_str(&content).with_context(|| {
        format!(
            "failed to parse TOML config file at {}",
            config_path.display()
        )
    })?;
    let mut config = expansion::expand_raw_config(raw_config)?;

    validation::validate_and_resolve_container_names(&mut config)?;
    validation::validate_swarm_targets(&config)?;

    Ok(config)
}

/// Loads raw config with from persisted or external state.
pub(crate) fn load_raw_config_with(
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<RawConfig> {
    let config_path = paths::resolve_config_path(config_path, project_root)?;
    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read config file at {}", config_path.display()))?;

    toml::from_str(&content).with_context(|| {
        format!(
            "failed to parse TOML config file at {}",
            config_path.display()
        )
    })
}

/// Saves configuration back to `.helm.toml`.
///
/// # Errors
///
/// Returns an error if the config path cannot be resolved or writing fails.
pub fn save_config_with(
    config: &Config,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<PathBuf> {
    let path = paths::resolve_config_path(config_path, project_root)?;
    let content = toml::to_string_pretty(config).context("failed to serialize config as TOML")?;

    std::fs::write(&path, content)
        .with_context(|| format!("failed to write config file at {}", path.display()))?;

    Ok(path)
}
