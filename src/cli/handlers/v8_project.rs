//! Shared strict v8 project resolution before singleton IPC dispatch.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::cli::dispatch::context::CliDispatchContext;
use crate::control_plane::RawProjectConfig;
use crate::control_plane::parse_project_config;

pub(super) struct V8Project {
    root: PathBuf,
    config: RawProjectConfig,
    services: BTreeSet<String>,
}

impl V8Project {
    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) const fn config(&self) -> &RawProjectConfig {
        &self.config
    }

    pub(super) fn has_service(&self, service: &str) -> bool {
        self.services.contains(service)
    }

    pub(super) fn service_names(&self) -> Vec<String> {
        self.services.iter().cloned().collect()
    }
}

pub(super) fn resolve_v8_project(context: &CliDispatchContext<'_>) -> Result<Option<V8Project>> {
    let Some((project_root, config_path)) = locate_project_config(context)? else {
        return Ok(None);
    };
    let metadata = fs::symlink_metadata(&config_path)
        .with_context(|| format!("failed to inspect {}", config_path.display()))?;
    if metadata.file_type().is_symlink() {
        bail!(
            "strict v8 configuration '{}' must not be a symbolic link",
            config_path.display()
        );
    }
    if !metadata.is_file() {
        bail!(
            "strict v8 configuration '{}' must be a regular file",
            config_path.display()
        );
    }
    if config_path.extension().and_then(|value| value.to_str()) != Some("yaml") {
        return Ok(None);
    }
    if config_path.file_name().and_then(|value| value.to_str()) != Some(".stackctl.yaml") {
        bail!("strict v8 configuration must be named .stackctl.yaml");
    }
    let source = fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;
    let config = parse_project_config(&source, &config_path)?;
    let root = canonical_directory(&project_root)?;
    let services = config.services().keys().cloned().collect();

    Ok(Some(V8Project {
        root,
        config,
        services,
    }))
}

pub(super) fn locate_project_config(
    context: &CliDispatchContext<'_>,
) -> Result<Option<(PathBuf, PathBuf)>> {
    if let Some(path) = context.config_path() {
        let root = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        return Ok(Some((root.to_path_buf(), path.to_path_buf())));
    }

    let start = context
        .project_root()
        .map(Path::to_path_buf)
        .map_or_else(std::env::current_dir, Ok)
        .context("failed to get current directory")?;
    let mut current = Some(start.as_path());
    while let Some(directory) = current {
        let yaml = directory.join(".stackctl.yaml");
        if yaml.exists() {
            return Ok(Some((directory.to_path_buf(), yaml)));
        }
        current = directory.parent();
    }

    Ok(None)
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;

    use clap::Parser;

    use crate::cli::args::Cli;
    use crate::cli::dispatch::context::CliDispatchContext;

    use super::resolve_v8_project;

    #[test]
    fn project_resolution_refuses_a_symbolic_link_config() {
        let root = std::env::temp_dir().join(format!(
            "stackctl-v8-project-symlink-{}",
            std::process::id()
        ));
        drop(fs::remove_dir_all(&root));
        let project = root.join("project");
        fs::create_dir_all(&project).expect("create project directory");
        let victim = root.join("outside.yaml");
        fs::write(
            &victim,
            "schema_version: 8\nservices:\n  app:\n    preset: laravel\n",
        )
        .expect("write outside config");
        symlink(&victim, project.join(".stackctl.yaml")).expect("create config symlink");
        let cli = Cli::parse_from([
            "stackctl",
            "--project-root",
            project.to_str().expect("project path"),
            "status",
        ]);
        let context = CliDispatchContext::from_cli(&cli);

        let error = match resolve_v8_project(&context) {
            Ok(_) => panic!("symlink config must fail closed"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("symbolic link"));
        assert!(error.to_string().contains(".stackctl.yaml"));

        fs::remove_dir_all(root).expect("remove project fixture");
    }
}
