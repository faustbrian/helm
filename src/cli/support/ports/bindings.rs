//! cli support port binding helpers.

use anyhow::Result;
use std::collections::HashSet;

use crate::config;

pub(crate) fn apply_runtime_binding(
    runtime_config: &mut config::Config,
    runtime_service: &config::ServiceConfig,
) -> Result<()> {
    let Some(existing) = runtime_config
        .service
        .iter_mut()
        .find(|service| service.name == runtime_service.name)
    else {
        anyhow::bail!(
            "service '{}' not found while applying runtime binding",
            runtime_service.name
        );
    };

    existing.host = runtime_service.host.clone();
    existing.port = runtime_service.port;
    existing.smtp_port = runtime_service.smtp_port;
    Ok(())
}

pub(crate) fn collect_service_host_ports(
    services: &[config::ServiceConfig],
) -> HashSet<(String, u16)> {
    let mut ports = HashSet::new();
    for service in services {
        for bound in service_host_ports(service) {
            ports.insert(bound);
        }
    }
    ports
}

pub(crate) fn insert_service_host_ports(
    used_ports: &mut HashSet<(String, u16)>,
    service: &config::ServiceConfig,
) {
    for bound in service_host_ports(service) {
        used_ports.insert(bound);
    }
}

pub(crate) fn service_host_ports(service: &config::ServiceConfig) -> Vec<(String, u16)> {
    let mut ports = Vec::with_capacity(2);
    let main = (service.host.clone(), service.port);
    ports.push(main.clone());
    if let Some(smtp_port) = service.smtp_port
        && main.1 != smtp_port
    {
        ports.push((service.host.clone(), smtp_port));
    }
    ports
}
