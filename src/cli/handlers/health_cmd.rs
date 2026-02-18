//! cli handlers health cmd module.
//!
//! Contains cli handlers health cmd logic used by Helm command workflows.

use anyhow::Result;

use super::service_scope::for_each_service;
use crate::{config, docker};

pub(crate) struct HandleHealthOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) kind: Option<config::Kind>,
    pub(crate) timeout: u64,
    pub(crate) interval: u64,
    pub(crate) retries: Option<u32>,
    pub(crate) parallel: usize,
}

pub(crate) fn handle_health(
    config: &config::Config,
    options: HandleHealthOptions<'_>,
) -> Result<()> {
    for_each_service(
        config,
        options.service,
        options.kind,
        options.parallel,
        |svc| docker::wait_until_healthy(svc, options.timeout, options.interval, options.retries),
    )
}
