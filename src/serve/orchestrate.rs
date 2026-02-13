//! High-level serve lifecycle orchestration.
//!
//! Coordinates runtime image resolution, container startup, optional CA trust,
//! and Caddy/hosts integration for externally reachable URLs.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::{
    configure_caddy, ensure_container_running, ensure_hosts_entry, remove_caddy_route,
    remove_container, resolve_caddy_ports, resolve_runtime_image, served_url, service_domain,
    trust_inner_container_ca,
};

/// Starts a serve target and configures Caddy reverse proxy with local TLS.
///
/// # Errors
///
/// Returns an error if Docker or Caddy orchestration fails.
pub fn run(
    target: &ServiceConfig,
    recreate: bool,
    trust_container_ca: bool,
    detached: bool,
    project_root: &Path,
    injected_env: &HashMap<String, String>,
    allow_rebuild: bool,
) -> Result<()> {
    let ports = resolve_caddy_ports()?;
    let runtime_image = resolve_runtime_image(target, allow_rebuild, injected_env)?;
    ensure_container_running(target, recreate, &runtime_image, project_root, injected_env)?;
    if trust_container_ca {
        trust_inner_container_ca(target, detached)?;
    }
    let url = if target.localhost_tls {
        format!("https://localhost:{}", target.port)
    } else {
        configure_caddy(target, ports)?;
        ensure_hosts_entry(target)?;
        served_url(service_domain(target)?, ports.https)
    };
    output::event(
        &target.name,
        LogLevel::Success,
        &format!("Serve target available at {url}"),
        Persistence::Persistent,
    );
    Ok(())
}

/// Stops and removes a serve target container and removes its Caddy route.
///
/// # Errors
///
/// Returns an error if Docker or Caddy orchestration fails.
pub fn down(target: &ServiceConfig) -> Result<()> {
    remove_container(target)?;
    if !target.localhost_tls {
        remove_caddy_route(target)?;
    }
    Ok(())
}

/// Computes the public URL for a serve target based on configured Caddy ports.
///
/// # Errors
///
/// Returns an error if Caddy port env values are invalid.
pub fn public_url(target: &ServiceConfig) -> Result<String> {
    if target.localhost_tls {
        return Ok(format!("https://localhost:{}", target.port));
    }
    let ports = resolve_caddy_ports()?;
    Ok(served_url(service_domain(target)?, ports.https))
}
