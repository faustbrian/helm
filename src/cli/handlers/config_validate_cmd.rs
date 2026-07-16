use anyhow::{Context, Result};
use std::path::Path;

/// Validates complete v8 desired state without contacting mutable runtime state.
pub(crate) fn handle_config_validate(path: Option<&Path>, quiet: bool) -> Result<()> {
    let path = match path {
        Some(path) => path.to_path_buf(),
        None => std::env::current_dir()?.join(".stackctl.yaml"),
    };
    let source = crate::control_plane::read_bounded_yaml_file(
        &path,
        crate::control_plane::MAX_PROJECT_CONFIG_BYTES,
    )
    .with_context(|| format!("failed to read {}", path.display()))?;
    let raw = crate::control_plane::parse_project_config(&source, &path)?;
    let project_directory = path
        .parent()
        .context("v8 configuration path must have a project directory")?;
    crate::control_plane::resolve_desired_project(raw, project_directory)
        .with_context(|| format!("invalid desired state in {}", path.display()))?;

    if !quiet {
        println!("valid: {}", path.display());
    }
    Ok(())
}
