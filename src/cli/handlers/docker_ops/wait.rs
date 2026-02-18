//! cli handlers docker ops wait module.
//!
//! Contains wait handler used by Helm command workflows.

use anyhow::Result;

use crate::{config, docker};

pub(crate) fn handle_wait(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    condition: Option<&str>,
    parallel: usize,
) -> Result<()> {
    super::run_for_each_docker_service(config, service, kind, parallel, |svc| {
        docker::wait(svc, condition)
    })
}
