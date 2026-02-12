use anyhow::Result;

use crate::config::ServiceConfig;

use super::{CaddyPorts, CaddyState};

mod fs_state;
mod process;
mod routes;
mod template;

pub use fs_state::caddy_access_log_path;
pub use process::trust_local_caddy_ca;

pub(super) fn configure_caddy(target: &ServiceConfig, ports: CaddyPorts) -> Result<()> {
    routes::configure_caddy(target, ports)
}

pub(super) fn remove_caddy_route(target: &ServiceConfig) -> Result<()> {
    routes::remove_caddy_route(target)
}

pub(super) fn domain_for_service(service: &ServiceConfig) -> Result<&str> {
    service
        .primary_domain()
        .ok_or_else(|| anyhow::anyhow!("app service '{}' is missing domain", service.name))
}

pub(super) fn domains_for_service(service: &ServiceConfig) -> Result<Vec<&str>> {
    let domains = service.resolved_domains();
    if domains.is_empty() {
        anyhow::bail!("app service '{}' is missing domain", service.name);
    }
    Ok(domains)
}

pub(super) fn render_caddyfile(
    state: &CaddyState,
    ports: CaddyPorts,
    access_log_path: &std::path::Path,
) -> String {
    template::render_caddyfile(state, ports, access_log_path)
}

pub(super) fn resolve_caddy_ports() -> Result<CaddyPorts> {
    template::resolve_caddy_ports()
}

pub(super) fn served_url(domain: &str, https_port: u16) -> String {
    template::served_url(domain, https_port)
}
