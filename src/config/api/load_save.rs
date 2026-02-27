//! config api load save module.
//!
//! Contains config api load save logic used by Helm command workflows.

use anyhow::Result;
use std::path::PathBuf;

use super::super::{Config, ContainerEngine, RawConfig, expansion, validation};
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

/// Loads the configured container runtime engine from raw config.
///
/// # Errors
///
/// Returns an error if path resolution, reading, or parsing fails.
pub fn load_container_engine_with(
    options: LoadConfigPathOptions<'_>,
) -> Result<Option<ContainerEngine>> {
    let raw = load_raw_config_with(options)?;
    Ok(raw.container_engine)
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

#[cfg(test)]
mod tests {
    use super::{LoadConfigPathOptions, load_container_engine_with};
    use crate::config::ContainerEngine;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("helm-load-save-{nanos}"));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn load_container_engine_prefers_configured_value() {
        let root = temp_root();
        let config_path = root.join(".helm.toml");
        fs::write(
            &config_path,
            "schema_version = 1\ncontainer_engine = \"podman\"\nservice = []\n",
        )
        .expect("write config");

        let engine =
            load_container_engine_with(LoadConfigPathOptions::new(Some(&config_path), None))
                .expect("load engine");

        assert_eq!(engine, Some(ContainerEngine::Podman));
    }
}
