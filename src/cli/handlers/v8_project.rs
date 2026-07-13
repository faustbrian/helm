//! Shared strict v8 project resolution before singleton IPC dispatch.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::cli::dispatch::context::CliDispatchContext;
use crate::config::{self, ProjectRootPathOptions};
use crate::control_plane::parse_project_config;

pub(super) struct V8Project {
    root: PathBuf,
    services: BTreeSet<String>,
}

impl V8Project {
    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn has_service(&self, service: &str) -> bool {
        self.services.contains(service)
    }
}

pub(super) fn resolve_v8_project(context: &CliDispatchContext<'_>) -> Result<Option<V8Project>> {
    let project_root = config::project_root_with(ProjectRootPathOptions::new(
        context.config_path(),
        context.project_root(),
    ))?;
    let config_path = match context.config_path() {
        Some(path) => path.to_path_buf(),
        None => config::config_path_in_dir(&project_root)?
            .context("Stackctl config disappeared while resolving the project")?,
    };
    if config_path.extension().and_then(|value| value.to_str()) != Some("yaml") {
        return Ok(None);
    }
    if config_path.file_name().and_then(|value| value.to_str()) != Some(".stackctl.yaml") {
        bail!("strict v8 configuration must be named .stackctl.yaml");
    }
    if context.runtime_env().is_some() {
        bail!("--env is not supported by strict v8 YAML configuration");
    }

    let source = fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;
    let config = parse_project_config(&source, &config_path)?;
    let root = canonical_directory(&project_root)?;
    let services = config.services().keys().cloned().collect();

    Ok(Some(V8Project { root, services }))
}

fn canonical_directory(path: &Path) -> Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("failed to canonicalize project path {}", path.display()))?;
    if !canonical.is_dir() {
        bail!("project path '{}' is not a directory", canonical.display());
    }
    Ok(canonical)
}
