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
    json: bool,
    all: bool,
    allow_empty: bool,
    filter: &[String],
) -> Result<()> {
    let effective_format = if json { Some("{{json .}}") } else { format };

    if all {
        output::event(
            "docker",
            LogLevel::Warn,
            "Streaming global Docker daemon events via --all",
            Persistence::Persistent,
        );
        return docker::events(since, until, effective_format, filter);
    }

    let selected = cli::support::selected_services(config, service, kind, None)?;
    if selected.is_empty() {
        if !allow_empty {
            anyhow::bail!("No services matched current filters; use --allow-empty to ignore");
        }
        output::event(
            "docker",
            LogLevel::Info,
            "No services matched filters; no events to stream",
            Persistence::Persistent,
        );
        return Ok(());
    }

    let container_names = selected
        .into_iter()
        .map(|svc| svc.container_name())
        .collect::<Result<Vec<String>>>()?;
    let all_filters = build_scoped_event_filters(filter, &container_names);
    output::event(
        "docker",
        LogLevel::Info,
        "Streaming Helm-scoped Docker daemon events",
        Persistence::Persistent,
    );
    docker::events(since, until, effective_format, &all_filters)
}

fn build_scoped_event_filters(base_filters: &[String], container_names: &[String]) -> Vec<String> {
    let mut all_filters = Vec::with_capacity(base_filters.len() + container_names.len() + 1);
    all_filters.extend(base_filters.iter().cloned());
    all_filters.push(format!("label={LABEL_MANAGED}={VALUE_MANAGED_TRUE}"));
    all_filters.extend(
        container_names
            .iter()
            .map(|container_name| format!("label={LABEL_CONTAINER}={container_name}")),
    );
    all_filters
}

#[cfg(test)]
mod tests {
    use super::build_scoped_event_filters;

    #[test]
    fn builds_label_scoped_event_filters() {
        let base = vec!["type=container".to_owned()];
        let containers = vec!["acme-db".to_owned(), "acme-app".to_owned()];
        let filters = build_scoped_event_filters(&base, &containers);
        assert!(filters.contains(&"type=container".to_owned()));
        assert!(filters.contains(&"label=com.helm.managed=true".to_owned()));
        assert!(filters.contains(&"label=com.helm.container=acme-db".to_owned()));
        assert!(filters.contains(&"label=com.helm.container=acme-app".to_owned()));
    }
}
