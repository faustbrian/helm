//! cli handlers docker ops stats module.
//!
//! Contains stats handler used by Helm command workflows.

use anyhow::Result;

use crate::{config, docker};

pub(crate) fn handle_stats(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    no_stream: bool,
    format: Option<&str>,
) -> Result<()> {
    super::run_for_selected_docker_services(config, service, kind, |svc| {
        docker::stats(svc, docker::StatsOptions { no_stream, format })
    })
}
