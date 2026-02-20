//! Shared TOML file I/O helpers for config API modules.

use anyhow::{Context, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

pub(super) fn resolve_config_path(
    options: super::project::ProjectRootPathOptions<'_>,
) -> Result<PathBuf> {
    super::super::paths::resolve_config_path(options.config_path, options.project_root)
}

pub(super) fn resolve_lockfile_path(
    options: super::project::ProjectRootPathOptions<'_>,
) -> Result<PathBuf> {
    super::super::paths::resolve_lockfile_path(options.config_path, options.project_root)
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
