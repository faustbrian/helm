//! Route-state mutation for Caddy-backed serve targets.

use anyhow::{Context, Result};

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::{
    CaddyPorts, caddy_access_log_path, domains_for_service, fs_state, process, render_caddyfile,
};

/// Adds/updates domains for a target and applies the resulting Caddy config.
pub(super) fn configure_caddy(target: &ServiceConfig, ports: CaddyPorts) -> Result<()> {
    let caddy_dir = fs_state::caddy_dir()?;
    let state_path = caddy_dir.join("sites.toml");
    let caddyfile_path = caddy_dir.join("Caddyfile");
    let access_log_path = caddy_access_log_path()?;

    let mut state = fs_state::read_caddy_state(&state_path)?;
    let upstream = format!("{}:{}", target.host, target.port);
    for domain in domains_for_service(target)? {
        state.routes.insert(domain.to_owned(), upstream.clone());
    }
    let caddyfile = render_caddyfile(&state, ports, &access_log_path);

    if crate::docker::is_dry_run() {
        print_dry_run(&state_path, &caddyfile_path);
        return Ok(());
    }

    std::fs::create_dir_all(&caddy_dir)
        .with_context(|| format!("failed to create {}", caddy_dir.display()))?;

    fs_state::write_caddy_state_and_file(&state_path, &caddyfile_path, &state, &caddyfile)?;
    process::ensure_caddy_installed()?;
    process::reload_or_start_caddy(&caddyfile_path)?;
    super::trust_local_caddy_ca()?;

    Ok(())
}

/// Removes target domains from route state and reapplies Caddy config.
pub(super) fn remove_caddy_route(target: &ServiceConfig) -> Result<()> {
    let ports = super::resolve_caddy_ports()?;
    let caddy_dir = fs_state::caddy_dir()?;
    let state_path = caddy_dir.join("sites.toml");
    let caddyfile_path = caddy_dir.join("Caddyfile");
    let access_log_path = caddy_access_log_path()?;

    let mut state = fs_state::read_caddy_state(&state_path)?;
    for domain in domains_for_service(target)? {
        state.routes.remove(domain);
    }
    let caddyfile = render_caddyfile(&state, ports, &access_log_path);

    if crate::docker::is_dry_run() {
        print_dry_run(&state_path, &caddyfile_path);
        return Ok(());
    }

    std::fs::create_dir_all(&caddy_dir)
        .with_context(|| format!("failed to create {}", caddy_dir.display()))?;
    fs_state::write_caddy_state_and_file(&state_path, &caddyfile_path, &state, &caddyfile)?;

    process::ensure_caddy_installed()?;
    process::reload_or_start_caddy(&caddyfile_path)?;
    Ok(())
}

/// Emits dry-run messages for Caddy state/config writes and reload command.
fn print_dry_run(state_path: &std::path::Path, caddyfile_path: &std::path::Path) {
    output::event(
        "caddy",
        LogLevel::Info,
        &format!("[dry-run] Write {}", state_path.display()),
        Persistence::Transient,
    );
    output::event(
        "caddy",
        LogLevel::Info,
        &format!("[dry-run] Write {}", caddyfile_path.display()),
        Persistence::Transient,
    );
    output::event(
        "caddy",
        LogLevel::Info,
        &format!(
            "[dry-run] caddy reload --config {} --adapter caddyfile",
            caddyfile_path.display()
        ),
        Persistence::Transient,
    );
}
