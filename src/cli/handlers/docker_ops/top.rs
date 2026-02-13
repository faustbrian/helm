//! cli handlers docker ops top module.
//!
//! Contains top handler used by Helm command workflows.

use anyhow::Result;

use crate::{cli, config, docker};

pub(super) fn handle_top(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    args: &[String],
) -> Result<()> {
    let selected = cli::support::selected_services(config, service, kind, None)?;
    for svc in selected {
        docker::top(svc, args)?;
    }
    Ok(())
}
