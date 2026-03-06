//! shared env-output path resolution helpers.

use anyhow::Result;
use std::path::{Path, PathBuf};

pub(crate) fn env_output_path(
    env_output: bool,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
    runtime_env: Option<&str>,
) -> Result<Option<PathBuf>> {
    if !env_output {
        return Ok(None);
    }

    super::default_env_path(config_path, project_root, None, runtime_env).map(Some)
}
