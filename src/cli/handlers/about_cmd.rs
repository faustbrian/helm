//! cli handlers about cmd module.
//!
//! Contains cli handlers about cmd logic used by Helm command workflows.

use anyhow::Result;
use std::path::Path;

use crate::{config, display};

/// Handles the `about` CLI command.
pub(crate) fn handle_about(
    config: &config::Config,
    runtime_env: Option<&str>,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<()> {
    let project_root = config::project_root_with(config_path, project_root)?;
    let config_path = config_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| project_root.join(".helm.toml"));

    display::print_about(config, &project_root, &config_path, runtime_env);
    Ok(())
}
