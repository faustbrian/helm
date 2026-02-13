//! cli handlers env cmd managed persist module.
//!
//! Contains cli handlers env cmd managed persist logic used by Helm command workflows.

use anyhow::Result;
use std::path::PathBuf;

use crate::output::{self, LogLevel, Persistence};
use crate::{config, docker};

pub(super) fn collect_persist_targets(
    selected: &[&config::ServiceConfig],
) -> Vec<(String, String, u16)> {
    selected
        .iter()
        .filter_map(|svc| {
            svc.container_name().ok().map(|container_name| {
                (
                    svc.name.clone(),
                    container_name,
                    svc.resolved_container_port(),
                )
            })
        })
        .collect()
}

pub(super) fn persist_runtime_host_ports(
    config: &mut config::Config,
    persist_targets: &[(String, String, u16)],
    quiet: bool,
    config_path: &Option<PathBuf>,
    project_root: &Option<PathBuf>,
) -> Result<()> {
    let mut changed_services = Vec::new();
    for (service_name, container_name, container_port) in persist_targets {
        if docker::inspect_status(container_name).as_deref() != Some("running") {
            continue;
        }

        let Some((host, port)) = docker::inspect_host_port_binding(container_name, *container_port)
        else {
            continue;
        };

        if config::update_service_host_port(config, service_name, &host, port)? {
            changed_services.push(service_name.clone());
        }
    }

    if !changed_services.is_empty() {
        let path =
            config::save_config_with(config, config_path.as_deref(), project_root.as_deref())?;
        if !quiet {
            output::event(
                "env",
                LogLevel::Success,
                &format!(
                    "Persisted runtime host/port to {} for: {}",
                    path.display(),
                    changed_services.join(", ")
                ),
                Persistence::Persistent,
            );
        }
    }

    Ok(())
}
