//! cli handlers restart cmd module.
//!
//! Contains cli handlers restart cmd logic used by Helm command workflows.

use anyhow::Result;

use super::service_scope::for_each_service_with_info;
use crate::{config, docker};

pub(crate) struct HandleRestartOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) kind: Option<config::Kind>,
    pub(crate) healthy: bool,
    pub(crate) timeout: u64,
    pub(crate) parallel: usize,
}

pub(crate) fn handle_restart(
    config: &config::Config,
    options: HandleRestartOptions<'_>,
) -> Result<()> {
    for_each_service_with_info(
        config,
        options.service,
        options.kind,
        options.parallel,
        false,
        "Restarting service",
        |svc| {
            docker::restart(svc)?;
            if options.healthy {
                docker::wait_until_healthy(svc, options.timeout, 2, None)?;
            }
            Ok(())
        },
    )
}
