//! Preflight preparation for up random-port runtime flow.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::cli::args::PortStrategyArg;
use crate::{cli, config};

use super::planning::{PlanRuntimeStartupOptions, RuntimePlan, plan_runtime_startup};

pub(super) type PreparedRandomPortsUp =
    cli::support::PreparedRandomPorts<(config::ServiceConfig, bool)>;

pub(super) struct PrepareRandomPortsUpOptions<'a> {
    pub(super) service: Option<&'a str>,
    pub(super) kind: Option<config::Kind>,
    pub(super) profile: Option<&'a str>,
    pub(super) force_random_ports: bool,
    pub(super) port_strategy: PortStrategyArg,
    pub(super) port_seed: Option<&'a str>,
    pub(super) env_output: bool,
    pub(super) runtime_env: Option<&'a str>,
    pub(super) workspace_root: &'a Path,
    pub(super) project_dependency_env: &'a HashMap<String, String>,
    pub(super) config_path: Option<&'a Path>,
    pub(super) project_root: Option<&'a Path>,
}

pub(super) fn prepare_random_ports_up(
    config: &config::Config,
    options: PrepareRandomPortsUpOptions<'_>,
) -> Result<PreparedRandomPortsUp> {
    cli::support::prepare_random_ports(
        config,
        options.env_output,
        options.runtime_env,
        options.config_path,
        options.project_root,
        |config| {
            let RuntimePlan { planned, app_env } = plan_runtime_startup(
                config,
                PlanRuntimeStartupOptions {
                    service: options.service,
                    kind: options.kind,
                    profile: options.profile,
                    force_random_ports: options.force_random_ports,
                    port_strategy: options.port_strategy,
                    port_seed: options.port_seed,
                    workspace_root: options.workspace_root,
                    runtime_env: options.runtime_env,
                    config_path: options.config_path,
                    project_root: options.project_root,
                    project_dependency_env: options.project_dependency_env,
                },
            )?;
            Ok((planned, app_env))
        },
    )
}
