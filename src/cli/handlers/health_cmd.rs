//! cli handlers health cmd module.
//!
//! Contains cli handlers health cmd logic used by Helm command workflows.

use anyhow::Result;

use crate::{cli, config, docker};

/// Handles the `health` CLI command.
pub(crate) fn handle_health(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    timeout: u64,
    interval: u64,
    retries: Option<u32>,
    parallel: usize,
) -> Result<()> {
    cli::support::for_each_service(config, service, kind, None, parallel, |svc| {
        docker::wait_until_healthy(svc, timeout, interval, retries)
    })
}
