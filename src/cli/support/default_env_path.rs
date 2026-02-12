use anyhow::Result;
use std::path::PathBuf;

use crate::config;

pub(crate) fn default_env_path(
    config_path: &Option<PathBuf>,
    project_root: &Option<PathBuf>,
    env_file: &Option<PathBuf>,
    runtime_env: Option<&str>,
) -> Result<PathBuf> {
    if let Some(path) = env_file {
        return Ok(path.clone());
    }

    let root = if config_path.is_none() && project_root.is_none() {
        config::project_root()?
    } else {
        config::project_root_with(config_path.as_deref(), project_root.as_deref())?
    };

    let env_file_name = config::default_env_file_name(runtime_env)?;
    Ok(root.join(env_file_name))
}
