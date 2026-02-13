//! cli handlers docker ops wait module.
//!
//! Contains wait handler used by Helm command workflows.

use anyhow::Result;

use crate::{cli, config, docker};

pub(super) fn handle_wait(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    condition: Option<&str>,
    parallel: usize,
) -> Result<()> {
    cli::support::for_each_service(config, service, kind, None, parallel, |svc| {
        docker::wait(svc, condition)
    })
}
