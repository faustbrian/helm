use anyhow::Result;

use crate::{cli, config, docker};

pub(crate) fn handle_exec(
    config: &config::Config,
    service: Option<&str>,
    tty: bool,
    no_tty: bool,
    command: &[String],
) -> Result<()> {
    if command.is_empty() {
        anyhow::bail!("No command specified. Usage: helm exec --service <name> -- <command>");
    }
    let svc = if service.is_some() {
        config::resolve_service(config, service)?
    } else if let Ok(app) = config::resolve_app_service(config, None) {
        app
    } else {
        config::resolve_service(config, None)?
    };
    docker::exec_command(svc, command, cli::support::resolve_tty(tty, no_tty))
}
