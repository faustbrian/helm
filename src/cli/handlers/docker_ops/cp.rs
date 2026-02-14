//! cli handlers docker ops cp module.
//!
//! Contains cp handler used by Helm command workflows.

use anyhow::Result;

use crate::{config, docker};

pub(super) fn handle_cp(
    config: &config::Config,
    source: &str,
    destination: &str,
    follow_link: bool,
    archive: bool,
) -> Result<()> {
    let source_resolved = resolve_cp_endpoint(config, source)?;
    let destination_resolved = resolve_cp_endpoint(config, destination)?;
    docker::cp(
        &source_resolved,
        &destination_resolved,
        follow_link,
        archive,
    )
}

fn resolve_cp_endpoint(config: &config::Config, endpoint: &str) -> Result<String> {
    let Some((prefix, path)) = endpoint.split_once(':') else {
        return Ok(endpoint.to_owned());
    };

    if prefix.is_empty() {
        return Ok(endpoint.to_owned());
    }

    let Ok(svc) = config::find_service(config, prefix) else {
        return Ok(endpoint.to_owned());
    };
    let container_name = svc.container_name()?;
    Ok(format!("{container_name}:{path}"))
}
