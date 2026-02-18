//! cli handlers docker ops kill module.
//!
//! Contains kill handler used by Helm command workflows.

use anyhow::Result;

use crate::{config, docker};

pub(crate) fn handle_kill(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    signal: Option<&str>,
    parallel: usize,
) -> Result<()> {
    super::run_for_each_docker_service(config, service, kind, parallel, |svc| {
        docker::kill(svc, signal)
    })
}
