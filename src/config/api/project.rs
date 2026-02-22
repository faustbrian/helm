//! config api project module.
//!
//! Contains config api project logic used by Helm command workflows.

use anyhow::Result;
use std::path::{Path, PathBuf};

use super::super::paths;

pub struct ProjectRootPathOptions<'a> {
    pub config_path: Option<&'a Path>,
    pub project_root: Option<&'a Path>,
    pub runtime_env: Option<&'a str>,
}

impl<'a> ProjectRootPathOptions<'a> {
    pub fn new(config_path: Option<&'a Path>, project_root: Option<&'a Path>) -> Self {
        Self {
            config_path,
            project_root,
            runtime_env: None,
        }
    }

    #[must_use]
    pub fn with_runtime_env(mut self, runtime_env: Option<&'a str>) -> Self {
        self.runtime_env = runtime_env;
        self
    }
}

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
pub fn project_root_with(options: ProjectRootPathOptions<'_>) -> Result<PathBuf> {
    paths::project_root_with(options.config_path, options.project_root)
}

/// Creates a starter `.helm.toml` in the current directory.
///
/// # Errors
///
/// Returns an error when file exists or cannot be written.
pub fn init_config() -> Result<PathBuf> {
    paths::init_config()
}
