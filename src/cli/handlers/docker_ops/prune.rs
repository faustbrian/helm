//! cli handlers docker ops prune module.
//!
//! Contains prune handler used by Helm command workflows.

use anyhow::Result;

use crate::output::{self, LogLevel, Persistence};
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
    if all && !force {
        anyhow::bail!("--all requires --force to avoid accidental global prune");
    }

    if all {
        output::event(
            "docker",
            LogLevel::Warn,
            "Pruning all stopped Docker containers via --all",
            Persistence::Persistent,
        );
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
