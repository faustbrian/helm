//! cli handlers docker ops inspect module.
//!
//! Contains inspect handler used by Helm command workflows.

use anyhow::Result;

use crate::{cli, config, docker};

pub(super) fn handle_inspect(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    format: Option<&str>,
    size: bool,
    object_type: Option<&str>,
) -> Result<()> {
    let selected = cli::support::selected_services(config, service, kind, None)?;
    for svc in selected {
        docker::inspect_container(svc, format, size, object_type)?;
    }
    Ok(())
}
