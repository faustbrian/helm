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

pub struct RunServeOptions<'a> {
    pub target: &'a ServiceConfig,
    pub recreate: bool,
    pub trust_container_ca: bool,
    pub detached: bool,
    pub project_root: &'a Path,
    pub injected_env: &'a HashMap<String, String>,
    pub allow_rebuild: bool,
}

/// Starts a serve target and configures Caddy reverse proxy with local TLS.
///
/// # Errors
///
/// Returns an error if Docker or Caddy orchestration fails.
pub fn run(options: RunServeOptions<'_>) -> Result<()> {
    let ports = resolve_caddy_ports()?;
    let runtime_image =
        resolve_runtime_image(options.target, options.allow_rebuild, options.injected_env)?;
    ensure_container_running(
        options.target,
        options.recreate,
        &runtime_image,
        options.project_root,
        options.injected_env,
    )?;
    if options.trust_container_ca {
        trust_inner_container_ca(options.target, options.detached)?;
    }
    let url = if options.target.localhost_tls {
        localhost_url(options.target)
    } else {
        configure_caddy(options.target, ports)?;
        ensure_hosts_entry(options.target)?;
        caddy_served_url(options.target, ports.https)?
    };
    output::event(
        &options.target.name,
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
        return Ok(localhost_url(target));
    }
    let ports = resolve_caddy_ports()?;
    caddy_served_url(target, ports.https)
}

fn localhost_url(target: &ServiceConfig) -> String {
    format!("https://localhost:{}", target.port)
}

fn caddy_served_url(target: &ServiceConfig, https_port: u16) -> Result<String> {
    Ok(served_url(service_domain(target)?, https_port))
}
