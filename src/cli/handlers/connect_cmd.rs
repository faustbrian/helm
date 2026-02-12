use anyhow::Result;

use crate::{cli, config, docker};

pub(crate) fn handle_connect(
    config: &config::Config,
    service: Option<&str>,
    tty: bool,
    no_tty: bool,
) -> Result<()> {
    let svc = config::resolve_service(config, service)?;
    docker::exec_interactive(svc, cli::support::resolve_tty(tty, no_tty))
}
