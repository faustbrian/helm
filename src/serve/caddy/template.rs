use anyhow::{Context, Result};
use std::path::Path;

use super::{CaddyPorts, CaddyState};

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
        caddyfile.push_str(&format!("    reverse_proxy {upstream}\n"));
        caddyfile.push_str("    tls internal\n");
        caddyfile.push_str("}\n\n");
    }

    caddyfile
}

pub(super) fn resolve_caddy_ports() -> Result<CaddyPorts> {
    let http = std::env::var("SILO_CADDY_HTTP_PORT")
        .ok()
        .map_or(Ok(80_u16), parse_port_env("SILO_CADDY_HTTP_PORT"))?;
    let https = std::env::var("SILO_CADDY_HTTPS_PORT")
        .ok()
        .map_or(Ok(443_u16), parse_port_env("SILO_CADDY_HTTPS_PORT"))?;

    Ok(CaddyPorts { http, https })
}

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

pub(super) fn served_url(domain: &str, https_port: u16) -> String {
    if https_port == 443 {
        return format!("https://{domain}");
    }

    format!("https://{domain}:{https_port}")
}
