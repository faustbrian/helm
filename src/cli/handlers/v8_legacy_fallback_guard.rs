//! Prevent strict v8 projects from entering legacy runtime dispatch.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::cli::args::Cli;
use crate::cli::dispatch::context::CliDispatchContext;
use crate::config;

use super::v8_project::resolve_v8_project;

pub(crate) fn reject_v8_legacy_fallback(
    _cli: &Cli,
    context: &CliDispatchContext<'_>,
) -> Result<()> {
    if v8_config_path(context)?.is_some() && resolve_v8_project(context)?.is_some() {
        bail!(
            "this command is not implemented for strict v8 and will not fall back to the v7 Docker or host-tooling runtime"
        );
    }

    Ok(())
}

fn v8_config_path(context: &CliDispatchContext<'_>) -> Result<Option<PathBuf>> {
    if let Some(path) = context.config_path() {
        return Ok(
            (path.extension().and_then(|value| value.to_str()) == Some("yaml"))
                .then(|| path.to_path_buf()),
        );
    }

    let start = match context.project_root() {
        Some(path) => path.to_path_buf(),
        None => std::env::current_dir().context("failed to get current directory")?,
    };
    find_v8_config_in_ancestors(&start)
}

fn find_v8_config_in_ancestors(start: &Path) -> Result<Option<PathBuf>> {
    let mut current = Some(start);
    while let Some(directory) = current {
        if let Some(path) = config::config_path_in_dir(directory)? {
            return Ok(
                (path.extension().and_then(|value| value.to_str()) == Some("yaml")).then_some(path),
            );
        }
        current = directory.parent();
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use clap::Parser;

    use crate::cli::args::Cli;
    use crate::cli::dispatch::context::CliDispatchContext;

    use super::reject_v8_legacy_fallback;

    static PROJECT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    fn project(config_name: &str, contents: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "stackctl-v8-fallback-{}-{}",
            std::process::id(),
            PROJECT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create project");
        fs::write(root.join(config_name), contents).expect("write config");
        root
    }

    #[test]
    fn strict_yaml_commands_never_fall_through_to_v7_runtime_dispatch() {
        let root = project(
            ".stackctl.yaml",
            "schema_version: 8\nservices:\n  app:\n    preset: app\n",
        );
        let cli = Cli::parse_from([
            "stackctl",
            "--project-root",
            root.to_str().expect("root"),
            "logs",
        ]);
        let context = CliDispatchContext::from_cli(&cli);

        let error = reject_v8_legacy_fallback(&cli, &context).expect_err("v8 boundary");

        assert!(error.to_string().contains("not implemented for strict v8"));
        assert!(error.to_string().contains("will not fall back"));
    }

    #[test]
    fn toml_projects_remain_available_to_the_v7_dispatcher() {
        let root = project(
            ".stackctl.toml",
            "schema_version = 1\nproject_type = \"project\"\nservice = []\nswarm = []\n",
        );
        let cli = Cli::parse_from([
            "stackctl",
            "--project-root",
            root.to_str().expect("root"),
            "logs",
        ]);
        let context = CliDispatchContext::from_cli(&cli);

        reject_v8_legacy_fallback(&cli, &context).expect("v7 fallback");
    }
}
