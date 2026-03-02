//! Caddy integration boundary for serve mode.
//!
//! Exposes high-level helpers used by orchestrators while keeping file/state,
//! template rendering, and process management in dedicated submodules.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::{CaddyPorts, CaddyState};

mod fs_state;
mod process;
mod routes;
mod template;

pub use fs_state::caddy_access_log_path;
pub use process::trust_local_caddy_ca;

/// Adds or updates Caddy routes for the given serve target.
pub(super) fn configure_caddy(target: &ServiceConfig, ports: CaddyPorts) -> Result<()> {
    routes::configure_caddy(target, ports)
}

/// Removes Caddy routes for the given serve target.
pub(super) fn remove_caddy_route(target: &ServiceConfig) -> Result<()> {
    routes::remove_caddy_route(target)
}

/// Returns the primary domain for a service, or an error if none is configured.
pub(super) fn domain_for_service(service: &ServiceConfig) -> Result<&str> {
    service
        .primary_domain()
        .ok_or_else(|| anyhow::anyhow!("app service '{}' is missing domain", service.name))
}

/// Returns all resolved domains for a service.
pub(super) fn domains_for_service(service: &ServiceConfig) -> Result<Vec<&str>> {
    let domains = service.resolved_domains();
    if domains.is_empty() {
        anyhow::bail!("app service '{}' is missing domain", service.name);
    }
    Ok(domains)
}

/// Renders a complete Caddyfile from current route state.
pub(super) fn render_caddyfile(
    state: &CaddyState,
    ports: CaddyPorts,
    access_log_path: &std::path::Path,
) -> String {
    template::render_caddyfile(state, ports, access_log_path)
}

/// Resolves Caddy HTTP/HTTPS ports from environment overrides.
pub(super) fn resolve_caddy_ports() -> Result<CaddyPorts> {
    template::resolve_caddy_ports()
}

/// Builds the externally-visible HTTPS URL for a served domain.
pub(super) fn served_url(domain: &str, https_port: u16) -> String {
    template::served_url(domain, https_port)
}
