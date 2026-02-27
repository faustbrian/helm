//! Shared random-port preflight preparation helpers.

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config;

pub(crate) struct PreparedRandomPorts<T> {
    pub(crate) planned: Vec<T>,
    pub(crate) app_env: HashMap<String, String>,
    pub(crate) env_path: Option<PathBuf>,
}

pub(crate) fn prepare_random_ports<T, F>(
    config: &config::Config,
    env_output: bool,
    runtime_env: Option<&str>,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
    planner: F,
) -> Result<PreparedRandomPorts<T>>
where
    F: FnOnce(&config::Config) -> Result<(Vec<T>, HashMap<String, String>)>,
{
    let env_path = super::env_output_path(env_output, config_path, project_root, runtime_env)?;
    let (planned, app_env) = planner(config)?;

    Ok(PreparedRandomPorts {
        planned,
        app_env,
        env_path,
    })
}
