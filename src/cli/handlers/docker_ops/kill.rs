//! cli handlers docker ops kill module.
//!
//! Contains kill handler used by Helm command workflows.

use anyhow::Result;

use crate::{cli, config, docker};

pub(super) fn handle_kill(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    signal: Option<&str>,
    parallel: usize,
) -> Result<()> {
    cli::support::for_each_service(config, service, kind, None, parallel, |svc| {
        docker::kill(svc, signal)
    })
}
