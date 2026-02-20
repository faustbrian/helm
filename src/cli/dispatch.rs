//! Top-level CLI dispatch pipeline.
//!
//! This is the single entrypoint that applies global CLI process settings
//! (`--no-color`, `--quiet`, `--dry-run`), runs setup-only commands that do not
//! require config loading, and then routes to primary/secondary command trees.

use anyhow::Result;

use super::args::Cli;
use crate::docker;
use crate::output;

mod bootstrap;
mod context;
mod primary;
mod secondary;

/// Executes one full CLI invocation.
///
/// Dispatch order matters:
/// 1. Apply global process-level flags.
/// 2. Handle setup commands that can run without config.
/// 3. Load and optionally runtime-patch config.
/// 4. Attempt primary dispatch, then fall through to secondary.
pub(crate) fn run(cli: Cli) -> Result<()> {
    if cli.no_color {
        colored::control::set_override(false);
    }

    output::init(cli.quiet);
    docker::set_dry_run(cli.dry_run);
    let dispatch_context = context::CliDispatchContext::from_cli(&cli);

    if bootstrap::handle_setup_commands(&cli, &dispatch_context)? {
        return Ok(());
    }

    let mut config = bootstrap::load_config_for_cli(&cli, &dispatch_context)?;
    if let Some(result) = primary::dispatch_primary(&cli, &mut config, &dispatch_context) {
        return result;
    }
    secondary::dispatch_secondary(&cli, &mut config, &dispatch_context)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::cli::args::Cli;
    use clap::Parser;

    fn minimal_config_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "helm-dispatch-run-{}-{}",
            std::process::id(),
            nanos
        ));
        drop(fs::remove_dir_all(&dir));
        fs::create_dir_all(&dir).expect("create temporary project root");
        let config_path = dir.join(".helm.toml");
        fs::write(
            &config_path,
            "schema_version = 1\nservice = []\nswarm = []\n",
        )
        .expect("write minimal config");
        dir
    }

    #[test]
    fn run_dispatches_about_from_full_pipeline() {
        let project_root = minimal_config_dir();
        let args = [
            "helm",
            "--project-root",
            project_root.to_str().expect("project root is valid utf-8"),
            "about",
        ];
        let cli = Cli::try_parse_from(args).expect("parse cli");
        let result = super::run(cli);
        assert!(result.is_ok());
    }

    #[test]
    fn run_dispatches_secondary_status_via_full_pipeline() {
        let project_root = minimal_config_dir();
        let args = [
            "helm",
            "--project-root",
            project_root.to_str().expect("project root is valid utf-8"),
            "status",
        ];
        let cli = Cli::parse_from(args);
        let result = super::run(cli);
        assert!(result.is_ok());
    }
}
