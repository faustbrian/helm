//! cli handlers docker ops unpause module.
//!
//! Contains unpause handler used by Helm command workflows.

use anyhow::Result;

use crate::{config, docker};

pub(crate) fn handle_unpause(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    parallel: usize,
) -> Result<()> {
    super::run_for_each_docker_service(config, service, kind, parallel, docker::unpause)
}
