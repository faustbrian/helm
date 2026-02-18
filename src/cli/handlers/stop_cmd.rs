//! cli handlers stop cmd module.
//!
//! Contains cli handlers stop cmd logic used by Helm command workflows.

use anyhow::Result;

use super::service_scope::for_each_service_with_info;
use crate::{config, docker};

pub(crate) fn handle_stop(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    parallel: usize,
    quiet: bool,
) -> Result<()> {
    for_each_service_with_info(
        config,
        service,
        kind,
        parallel,
        quiet,
        "Stopping service",
        docker::stop,
    )
}
