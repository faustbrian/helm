//! Shared service selection/execution helpers for CLI handlers.

use anyhow::Result;

use super::log;
use crate::{cli, config};

pub(super) fn for_each_service<F>(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    parallel: usize,
    callback: F,
) -> Result<()>
where
    F: Fn(&config::ServiceConfig) -> Result<()> + Send + Sync,
{
    cli::support::ServiceScope::new(config, service, kind).for_each(parallel, callback)
}

pub(super) fn selected_services<'a>(
    config: &'a config::Config,
    service: Option<&'a str>,
    kind: Option<config::Kind>,
) -> Result<Vec<&'a config::ServiceConfig>> {
    cli::support::ServiceScope::new(config, service, kind).selected()
}

pub(super) fn for_selected_services(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    callback: impl Fn(&config::ServiceConfig) -> Result<()>,
) -> Result<()> {
    cli::support::ServiceScope::new(config, service, kind).for_selected(callback)
}

pub(super) fn for_each_service_with_info<F>(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    parallel: usize,
    quiet: bool,
    message: &str,
    callback: F,
) -> Result<()>
where
    F: Fn(&config::ServiceConfig) -> Result<()> + Send + Sync,
{
    for_each_service(config, service, kind, parallel, |service| {
        log::info_if_not_quiet(quiet, &service.name, message);
        callback(service)
    })
}
