//! cli handlers docker ops pause module.
//!
//! Contains pause handler used by Helm command workflows.

use anyhow::Result;

use crate::{cli, config, docker};

pub(super) fn handle_pause(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    parallel: usize,
) -> Result<()> {
    cli::support::for_each_service(config, service, kind, None, parallel, |svc| {
        docker::pause(svc)
    })
}
