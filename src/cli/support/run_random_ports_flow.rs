//! Shared random-port runtime execution and persistence flow.

use anyhow::Result;
use std::path::Path;

use crate::config;

use super::{persist_random_runtime_ports, run_services_with_app_last};

pub(crate) struct RandomPortsPersistenceOptions<'a> {
    pub(crate) env_path: Option<&'a Path>,
    pub(crate) save_ports: bool,
    pub(crate) command: &'a str,
    pub(crate) quiet: bool,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
}

pub(crate) fn run_random_ports_flow<'a, T, FIsApp, FRun, FService>(
    planned: &'a [T],
    parallel: usize,
    is_app: FIsApp,
    runtime_runner: FRun,
    service_from_item: FService,
    config_data: &mut config::Config,
    persistence: RandomPortsPersistenceOptions<'_>,
) -> Result<()>
where
    T: Sync,
    FIsApp: Fn(&T) -> bool + Sync,
    FRun: Fn(&T) -> Result<()> + Sync + Send,
    FService: Fn(&'a T) -> &'a config::ServiceConfig,
{
    run_services_with_app_last(parallel, planned, is_app, runtime_runner)?;

    persist_random_runtime_ports(
        planned.iter().map(service_from_item),
        config_data,
        persistence.env_path,
        persistence.save_ports,
        persistence.command,
        persistence.quiet,
        persistence.config_path,
        persistence.project_root,
    )?;
    Ok(())
}
