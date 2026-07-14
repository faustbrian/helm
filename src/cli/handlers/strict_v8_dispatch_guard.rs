//! Prevent project commands from entering removed pre-v8 runtime dispatch.

use anyhow::{Result, bail};

use super::v8_project::{locate_project_config, resolve_v8_project};
use crate::cli::args::Cli;
use crate::cli::dispatch::context::CliDispatchContext;

pub(crate) fn enforce_strict_v8_dispatch(
    _cli: &Cli,
    context: &CliDispatchContext<'_>,
) -> Result<()> {
    let Some((_, path)) = locate_project_config(context)? else {
        return Ok(());
    };
    if path.extension().and_then(|value| value.to_str()) != Some("yaml") {
        bail!(
            "pre-v8 config '{}' is unsupported; create a new `.stackctl.yaml` for a clean v8 installation",
            path.display()
        );
    }
    if resolve_v8_project(context)?.is_some() {
        bail!("this command is not implemented for strict v8 and has no compatibility runtime");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use clap::Parser;

    use crate::cli::args::Cli;
    use crate::cli::dispatch::context::CliDispatchContext;

    use super::enforce_strict_v8_dispatch;

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
    fn strict_yaml_commands_never_enter_removed_runtime_dispatch() {
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

        let error = enforce_strict_v8_dispatch(&cli, &context).expect_err("v8 boundary");

        assert!(error.to_string().contains("not implemented for strict v8"));
        assert!(error.to_string().contains("no compatibility runtime"));
    }

    #[test]
    fn toml_projects_are_rejected_as_unsupported() {
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

        let error = enforce_strict_v8_dispatch(&cli, &context)
            .expect_err("pre-v8 config must not enter runtime dispatch");
        assert!(error.to_string().contains("pre-v8 config"));
        assert!(error.to_string().contains("clean v8 installation"));
    }
}
