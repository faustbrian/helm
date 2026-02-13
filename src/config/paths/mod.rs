//! config paths module.
//!
//! Contains config paths logic used by Helm command workflows.

use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};

mod search;
mod template;

use search::{find_config_file, find_config_in_path, find_project_root};
use template::default_config_template;

pub(super) fn project_root() -> Result<PathBuf> {
    project_root_with(None, None)
}

pub(super) fn project_root_with(
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<PathBuf> {
    if let Some(path) = config_path {
        return path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| anyhow!("config path '{}' has no parent directory", path.display()));
    }

    if let Some(root) = project_root {
        return find_project_root(root);
    }

    let current_dir = std::env::current_dir().context("failed to get current directory")?;
    find_project_root(&current_dir)
}

pub(super) fn init_config() -> Result<PathBuf> {
    let current_dir = std::env::current_dir().context("failed to get current directory")?;
    let config_path = current_dir.join(".helm.toml");

    if config_path.exists() {
        anyhow::bail!(".helm.toml already exists in {}", current_dir.display());
    }

    let project_name = current_dir
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("my-app");
    std::fs::write(&config_path, default_config_template(project_name))
        .with_context(|| format!("failed to write {}", config_path.display()))?;

    Ok(config_path)
}

/// Resolves config path using configured inputs and runtime state.
pub(super) fn resolve_config_path(
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<PathBuf> {
    if let Some(path) = config_path {
        if !path.exists() {
            anyhow::bail!("config file not found at {}", path.display());
        }
        return Ok(path.to_path_buf());
    }

    if let Some(root) = project_root {
        return find_config_in_path(root);
    }

    find_config_file()
}

/// Resolves lockfile path using configured inputs and runtime state.
pub(super) fn resolve_lockfile_path(
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<PathBuf> {
    Ok(project_root_with(config_path, project_root)?.join(".helm.lock.toml"))
}
