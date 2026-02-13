//! cli handlers node cmd module.
//!
//! Contains cli handlers node cmd logic used by Helm command workflows.

use anyhow::Result;
use std::path::Path;

use crate::{cli, config, env, serve};

/// Handles the `node` CLI command.
pub(crate) fn handle_node(
    config: &config::Config,
    service: Option<&str>,
    manager_bin: &str,
    tty: bool,
    no_tty: bool,
    command: &[String],
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<()> {
    if command.is_empty() {
        anyhow::bail!(
            "No package manager command specified. Usage: helm node [--manager bun|npm|pnpm|yarn] [--service <name>] -- <command>"
        );
    }
    let serve_target = config::resolve_app_service(config, service)?;
    let workspace_root = config::project_root_with(config_path, project_root)?;
    let inferred_env = env::inferred_app_env(config);
    let mut full_command = vec![manager_bin.to_owned()];
    full_command.extend(command.iter().cloned());
    serve::exec_or_run_command(
        serve_target,
        &full_command,
        cli::support::resolve_tty(tty, no_tty),
        &workspace_root,
        &inferred_env,
    )
}
