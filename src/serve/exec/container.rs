//! `docker exec` command execution helpers for serve containers.

use anyhow::Result;

use crate::config::ServiceConfig;
use crate::docker::build_exec_args;

/// Executes a command inside the app container.
///
/// # Errors
///
/// Returns an error if command execution fails.
pub(super) fn exec_command(target: &ServiceConfig, command: &[String], tty: bool) -> Result<()> {
    if command.is_empty() {
        anyhow::bail!("no command specified");
    }

    let container_name = target.container_name()?;
    let args = build_exec_args(&container_name, command, tty);

    super::docker_cmd::run_or_log_docker(
        &target.name,
        &args,
        "failed to execute artisan command in serve container",
        &format!("command failed in container '{container_name}'"),
    )
}
