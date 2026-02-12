use anyhow::Result;
use std::path::{Path, PathBuf};

use super::super::paths;

/// Returns the directory containing the `.helm.toml` file.
///
/// # Errors
///
/// Returns an error if the project root cannot be determined.
pub fn project_root() -> Result<PathBuf> {
    paths::project_root()
}

/// Returns the directory containing `.helm.toml`, with optional overrides.
///
/// # Errors
///
/// Returns an error if the root cannot be determined.
pub fn project_root_with(
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<PathBuf> {
    paths::project_root_with(config_path, project_root)
}

/// Creates a starter `.helm.toml` in the current directory.
///
/// # Errors
///
/// Returns an error when file exists or cannot be written.
pub fn init_config() -> Result<PathBuf> {
    paths::init_config()
}
