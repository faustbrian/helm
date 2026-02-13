//! cli handlers docker ops stats module.
//!
//! Contains stats handler used by Helm command workflows.

use anyhow::Result;

use crate::{cli, config, docker};

pub(super) fn handle_stats(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    no_stream: bool,
    format: Option<&str>,
) -> Result<()> {
    let selected = cli::support::selected_services(config, service, kind, None)?;
    for svc in selected {
        docker::stats(svc, no_stream, format)?;
    }
    Ok(())
}
