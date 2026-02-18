//! cli handlers docker ops top module.
//!
//! Contains top handler used by Helm command workflows.

use anyhow::Result;

use crate::{config, docker};

pub(crate) fn handle_top(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    args: &[String],
) -> Result<()> {
    super::run_for_selected_docker_services(config, service, kind, |svc| docker::top(svc, args))
}
