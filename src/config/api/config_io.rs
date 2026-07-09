//! Shared config file I/O helpers for config API modules.

use anyhow::{Context, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConfigFormat {
    Toml,
    Yaml,
}

pub(super) fn resolve_config_path(
    options: super::project::ProjectRootPathOptions<'_>,
) -> Result<PathBuf> {
    super::super::paths::resolve_config_path(
        options.config_path,
        options.project_root,
        options.runtime_env,
    )
}

pub(super) fn resolve_lockfile_path(
    options: super::project::ProjectRootPathOptions<'_>,
) -> Result<PathBuf> {
    super::super::paths::resolve_lockfile_path(options.config_path, options.project_root)
}

pub(super) fn read_config_file<T>(path: &Path, read_label: &str, parse_label: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {read_label} at {}", path.display()))?;

    match config_format_from_path(path)? {
        ConfigFormat::Toml => toml::from_str(&content).with_context(|| {
            format!(
                "failed to parse {parse_label} as TOML at {}",
                path.display()
            )
        }),
        ConfigFormat::Yaml => serde_yaml::from_str(&content).with_context(|| {
            format!(
                "failed to parse {parse_label} as YAML at {}",
                path.display()
            )
        }),
    }
}

pub(super) fn write_config_file<T>(
    path: &Path,
    value: &T,
    serialize_label: &str,
    write_label: &str,
) -> Result<()>
where
    T: Serialize,
{
    let content = match config_format_from_path(path)? {
        ConfigFormat::Toml => toml::to_string_pretty(value)
            .with_context(|| format!("failed to serialize {serialize_label} as TOML"))?,
        ConfigFormat::Yaml => serde_yaml::to_string(value)
            .with_context(|| format!("failed to serialize {serialize_label} as YAML"))?,
    };

    std::fs::write(path, content)
        .with_context(|| format!("failed to write {write_label} at {}", path.display()))
}

pub(super) fn read_toml_file<T>(path: &Path, read_label: &str, parse_label: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {read_label} at {}", path.display()))?;
    toml::from_str(&content)
        .with_context(|| format!("failed to parse {parse_label} at {}", path.display()))
}

pub(super) fn write_toml_file<T>(
    path: &Path,
    value: &T,
    serialize_label: &str,
    write_label: &str,
) -> Result<()>
where
    T: Serialize,
{
    let content = toml::to_string_pretty(value)
        .with_context(|| format!("failed to serialize {serialize_label} as TOML"))?;
    std::fs::write(path, content)
        .with_context(|| format!("failed to write {write_label} at {}", path.display()))
}

fn config_format_from_path(path: &Path) -> Result<ConfigFormat> {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        anyhow::bail!("unsupported config file path {}", path.display());
    };

    if name.ends_with(".toml") {
        return Ok(ConfigFormat::Toml);
    }

    if name.ends_with(".yaml") {
        return Ok(ConfigFormat::Yaml);
    }

    anyhow::bail!(
        "unsupported config file extension for {} (supported: .toml, .yaml)",
        path.display()
    )
}
