//! cli handlers docker ops port module.
//!
//! Contains port handler used by Helm command workflows.

use anyhow::Result;

use crate::{cli, config, docker};

pub(super) fn handle_port(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    private_port: Option<&str>,
) -> Result<()> {
    let selected = cli::support::selected_services(config, service, kind, None)?;
    for svc in selected {
        docker::port(svc, private_port)?;
    }
    Ok(())
}
