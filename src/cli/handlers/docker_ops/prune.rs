//! cli handlers docker ops prune module.
//!
//! Contains prune handler used by Helm command workflows.

use anyhow::Result;

use crate::{cli, config, docker};

pub(super) fn handle_prune(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    parallel: usize,
    all: bool,
    force: bool,
    filter: &[String],
) -> Result<()> {
    if all {
        return docker::prune(force, filter);
    }

    if force {
        anyhow::bail!("--force can only be used with --all");
    }
    if !filter.is_empty() {
        anyhow::bail!("--filter can only be used with --all");
    }

    cli::support::for_each_service(config, service, kind, None, parallel, |svc| {
        docker::prune_stopped_container(svc)
    })
}
