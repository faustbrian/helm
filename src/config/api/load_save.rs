//! config api load save module.
//!
//! Contains config api load save logic used by Helm command workflows.

use anyhow::Result;
use std::path::PathBuf;

use super::super::{Config, RawConfig, expansion, validation};
use super::project::ProjectRootPathOptions;

pub type RawConfigPathOptions<'a> = ProjectRootPathOptions<'a>;
pub type LoadConfigPathOptions<'a> = ProjectRootPathOptions<'a>;
pub type SaveConfigPathOptions<'a> = ProjectRootPathOptions<'a>;

/// Loads the configuration file using default discovery.
///
/// # Errors
///
/// Returns an error if the config cannot be found, read, or parsed.
pub fn load_config() -> Result<Config> {
    load_config_with(LoadConfigPathOptions::new(None, None))
}

/// Loads configuration from explicit config path or project root override.
///
/// # Errors
///
/// Returns an error if path resolution, reading, or parsing fails.
pub fn load_config_with(options: LoadConfigPathOptions<'_>) -> Result<Config> {
    let config_path = super::toml_io::resolve_config_path(options)?;
    let raw_config: RawConfig =
        super::toml_io::read_toml_file(&config_path, "config file", "TOML config file")?;
    let mut config = expansion::expand_raw_config(raw_config)?;

    validation::validate_and_resolve_container_names(&mut config)?;
    validation::validate_swarm_targets(&config)?;

    Ok(config)
}

/// Loads raw config with from persisted or external state.
pub(crate) fn load_raw_config_with(options: RawConfigPathOptions<'_>) -> Result<RawConfig> {
    let config_path = super::toml_io::resolve_config_path(options)?;
    super::toml_io::read_toml_file(&config_path, "config file", "TOML config file")
}

/// Saves configuration back to `.helm.toml`.
///
/// # Errors
///
/// Returns an error if the config path cannot be resolved or writing fails.
pub fn save_config_with(config: &Config, options: SaveConfigPathOptions<'_>) -> Result<PathBuf> {
    let path = super::toml_io::resolve_config_path(options)?;
    super::toml_io::write_toml_file(&path, config, "config", "config file")?;
    Ok(path)
}
