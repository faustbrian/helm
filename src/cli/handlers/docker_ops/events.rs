//! cli handlers docker ops events module.
//!
//! Contains events handler used by Helm command workflows.

use anyhow::Result;

use crate::docker::{LABEL_CONTAINER, LABEL_MANAGED, VALUE_MANAGED_TRUE};
use crate::output::{self, LogLevel, Persistence};
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
        output::event(
            "docker",
            LogLevel::Warn,
            "Streaming global Docker daemon events via --all",
            Persistence::Persistent,
        );
        return docker::events(since, until, format, filter);
    }

    let selected = cli::support::selected_services(config, service, kind, None)?;
    if selected.is_empty() {
        output::event(
            "docker",
            LogLevel::Info,
            "No services matched filters; no events to stream",
            Persistence::Persistent,
        );
        return Ok(());
    }

    let container_labels: Vec<String> = selected
        .into_iter()
        .map(|svc| svc.container_name())
        .collect::<Result<Vec<String>>>()?
        .into_iter()
        .map(|container_name| format!("label={LABEL_CONTAINER}={container_name}"))
        .collect();

    let mut all_filters = Vec::with_capacity(filter.len() + container_labels.len() + 1);
    all_filters.extend(filter.iter().cloned());
    all_filters.push(format!("label={LABEL_MANAGED}={VALUE_MANAGED_TRUE}"));
    all_filters.extend(container_labels);
    output::event(
        "docker",
        LogLevel::Info,
        "Streaming Helm-scoped Docker daemon events",
        Persistence::Persistent,
    );
    docker::events(since, until, format, &all_filters)
}
