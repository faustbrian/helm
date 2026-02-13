//! cli handlers docker ops events module.
//!
//! Contains events handler used by Helm command workflows.

use anyhow::Result;

use crate::{cli, config, docker};

pub(super) fn handle_events(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    since: Option<&str>,
    until: Option<&str>,
    format: Option<&str>,
    all: bool,
    filter: &[String],
) -> Result<()> {
    if all {
        return docker::events(since, until, format, filter);
    }

    let selected = cli::support::selected_services(config, service, kind, None)?;
    let container_filters: Vec<String> = selected
        .into_iter()
        .map(|svc| svc.container_name())
        .collect::<Result<Vec<String>>>()?
        .into_iter()
        .map(|container_name| format!("container={container_name}"))
        .collect();

    let mut all_filters = Vec::with_capacity(filter.len() + container_filters.len());
    all_filters.extend(filter.iter().cloned());
    all_filters.extend(container_filters);
    docker::events(since, until, format, &all_filters)
}
