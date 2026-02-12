use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};

pub(super) fn find_config_file() -> Result<PathBuf> {
    let current_dir = std::env::current_dir().context("failed to get current directory")?;

    find_config_in_path(&current_dir)
}

pub(super) fn find_project_root(start: &Path) -> Result<PathBuf> {
    let mut current = start;

    loop {
        if current.join(".helm.toml").exists() {
            return Ok(current.to_path_buf());
        }

        current = current.parent().ok_or_else(|| {
            anyhow!(".helm.toml not found in current directory or any parent directory")
        })?;
    }
}

pub(super) fn find_config_in_path(start_path: &Path) -> Result<PathBuf> {
    let mut current = start_path;

    loop {
        let config_path = current.join(".helm.toml");
        if config_path.exists() {
            return Ok(config_path);
        }

        current = current.parent().ok_or_else(|| {
            anyhow!(".helm.toml not found in current directory or any parent directory")
        })?;
    }
}
