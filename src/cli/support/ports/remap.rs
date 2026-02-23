//! cli support random port remap helpers.

use anyhow::Result;
use std::collections::HashSet;

use crate::config;

use super::ServicePortRemap;
use super::allocation::random_unused_port;
use super::bindings::insert_service_host_ports;

pub(crate) fn remap_random_ports(
    service: &mut config::ServiceConfig,
    used_ports: &mut HashSet<(String, u16)>,
    remap_smtp: bool,
) -> Result<ServicePortRemap> {
    let original_port = service.port;
    let original_smtp_port = service.smtp_port;
    let mut remap = ServicePortRemap::default();

    service.port = random_unused_port(&service.host, used_ports)?;
    insert_service_host_ports(used_ports, service);
    remap.main = service.port != original_port;

    let remapped_smtp = if remap_smtp {
        if let Some(original_smtp_port) = original_smtp_port {
            let smtp_port = random_unused_port(&service.host, used_ports)?;
            service.smtp_port = Some(smtp_port);
            insert_service_host_ports(used_ports, service);
            Some(smtp_port != original_smtp_port)
        } else {
            None
        }
    } else {
        None
    };

    remap.smtp = remapped_smtp.unwrap_or(false);

    Ok(remap)
}
