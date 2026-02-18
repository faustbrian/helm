//! cli handlers docker ops pause module.
//!
//! Contains pause handler used by Helm command workflows.

use anyhow::Result;

use crate::{config, docker};

pub(crate) fn handle_pause(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    parallel: usize,
) -> Result<()> {
    super::run_for_each_docker_service(config, service, kind, parallel, docker::pause)
}
