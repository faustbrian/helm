//! up random-ports planning helpers.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::cli::args::PortStrategyArg;
use crate::{cli, config};

use super::port_assignment::{
    assign_runtime_port, effective_port_seed, explicit_port_service_names, should_randomize_port,
};

pub(super) struct RuntimePlan {
    pub(super) planned: Vec<(config::ServiceConfig, bool)>,
    pub(super) app_env: HashMap<String, String>,
}

pub(super) struct PlanRuntimeStartupOptions<'a> {
    pub(super) service: Option<&'a str>,
    pub(super) kind: Option<config::Kind>,
    pub(super) profile: Option<&'a str>,
    pub(super) force_random_ports: bool,
    pub(super) port_strategy: PortStrategyArg,
    pub(super) port_seed: Option<&'a str>,
    pub(super) workspace_root: &'a Path,
    pub(super) runtime_env: Option<&'a str>,
    pub(super) config_path: Option<&'a Path>,
    pub(super) project_root: Option<&'a Path>,
    pub(super) project_dependency_env: &'a HashMap<String, String>,
}

pub(super) fn plan_runtime_startup(
    config: &config::Config,
    options: PlanRuntimeStartupOptions<'_>,
) -> Result<RuntimePlan> {
    let (explicit_port_services, explicit_smtp_services) =
        explicit_port_service_names(options.config_path, options.project_root)?;
    let mut runtime_config = config.clone();
    let selected: Vec<config::ServiceConfig> =
        cli::support::select_up_targets(config, options.service, options.kind, options.profile)?
            .into_iter()
            .cloned()
            .collect();
    let mut used_ports = cli::support::collect_service_host_ports(&runtime_config.service);
    let seed = effective_port_seed(
        options.workspace_root,
        options.runtime_env,
        options.port_seed,
    );

    let mut planned = Vec::new();
    for mut runtime in selected {
        let uses_random_port = should_randomize_port(
            &explicit_port_services,
            &runtime.name,
            options.force_random_ports,
        );
        if uses_random_port {
            runtime.port = assign_runtime_port(
                &runtime,
                options.port_strategy,
                &seed,
                &mut used_ports,
                "port",
            )?;
        }
        if runtime.driver == config::Driver::Mailhog
            && should_randomize_port(
                &explicit_smtp_services,
                &runtime.name,
                options.force_random_ports,
            )
        {
            runtime.smtp_port = Some(assign_runtime_port(
                &runtime,
                options.port_strategy,
                &seed,
                &mut used_ports,
                "smtp_port",
            )?);
        }
        cli::support::insert_service_host_ports(&mut used_ports, &runtime);
        cli::support::apply_runtime_binding(&mut runtime_config, &runtime)?;
        planned.push((runtime, uses_random_port));
    }

    let app_env = cli::support::runtime_app_env(&runtime_config, options.project_dependency_env);

    Ok(RuntimePlan { planned, app_env })
}
