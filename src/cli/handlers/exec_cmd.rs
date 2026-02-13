//! cli handlers exec cmd module.
//!
//! Contains cli handlers exec cmd logic used by Helm command workflows.

use anyhow::Result;

use crate::{cli, config, docker};

/// Handles the `exec` CLI command.
pub(crate) fn handle_exec(
    config: &config::Config,
    service: Option<&str>,
    tty: bool,
    no_tty: bool,
    command: &[String],
) -> Result<()> {
    let svc = if service.is_some() {
        config::resolve_service(config, service)?
    } else if let Ok(app) = config::resolve_app_service(config, None) {
        app
    } else {
        config::resolve_service(config, None)?
    };
    if command.is_empty() {
        return docker::exec_interactive(svc, cli::support::resolve_tty(tty, no_tty));
    }
    docker::exec_command(svc, command, cli::support::resolve_tty(tty, no_tty))
}
