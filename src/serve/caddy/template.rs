//! Caddyfile rendering and port parsing helpers.

use anyhow::{Context, Result};
use std::path::Path;

use super::{CaddyPorts, CaddyState};

/// Renders the full Caddyfile for all active routes.
///
/// The global block sets fixed HTTP/HTTPS ports and file-based access logging.
pub(super) fn render_caddyfile(
    state: &CaddyState,
    ports: CaddyPorts,
    access_log_path: &Path,
) -> String {
    let mut caddyfile = String::new();
    caddyfile.push_str("{\n");
    caddyfile.push_str("    auto_https disable_redirects\n");
    caddyfile.push_str(&format!("    http_port {}\n", ports.http));
    caddyfile.push_str(&format!("    https_port {}\n", ports.https));
    caddyfile.push_str("    log {\n");
    caddyfile.push_str(&format!(
        "        output file {}\n",
        access_log_path.display()
    ));
    caddyfile.push_str("        format console\n");
    caddyfile.push_str("    }\n");
    caddyfile.push_str("}\n\n");

    for (domain, upstream) in &state.routes {
        caddyfile.push_str(&format!("{domain} {{\n"));
        caddyfile.push_str(&format!("    reverse_proxy {upstream} {{\n"));
        caddyfile.push_str("        header_up X-Forwarded-Proto https\n");
        caddyfile.push_str("        header_up X-Forwarded-Port {http.request.port}\n");
        caddyfile.push_str("        header_up X-Forwarded-Host {host}\n");
        caddyfile.push_str("    }\n");
        caddyfile.push_str("    tls internal\n");
        caddyfile.push_str("}\n\n");
    }

    caddyfile
}

// TODO: resolve from config
/// Resolves Caddy ports from `HELM_CADDY_{HTTP,HTTPS}_PORT`.
///
/// Defaults to `80/443` when env vars are unset.
pub(super) fn resolve_caddy_ports() -> Result<CaddyPorts> {
    let http = std::env::var("HELM_CADDY_HTTP_PORT")
        .ok()
        .map_or(Ok(80_u16), parse_port_env("HELM_CADDY_HTTP_PORT"))?;
    let https = std::env::var("HELM_CADDY_HTTPS_PORT")
        .ok()
        .map_or(Ok(443_u16), parse_port_env("HELM_CADDY_HTTPS_PORT"))?;

    Ok(CaddyPorts { http, https })
}

/// Returns a parser for validating non-zero `u16` port values.
fn parse_port_env(var_name: &'static str) -> impl FnOnce(String) -> Result<u16> {
    move |raw| {
        let port = raw
            .parse::<u16>()
            .with_context(|| format!("{var_name} must be a valid u16 port number"))?;
        if port == 0 {
            anyhow::bail!("{var_name} must be >= 1");
        }
        Ok(port)
    }
}

/// Formats a public HTTPS URL, omitting the port for default `443`.
pub(super) fn served_url(domain: &str, https_port: u16) -> String {
    if https_port == 443 {
        return format!("https://{domain}");
    }

    format!("https://{domain}:{https_port}")
}
