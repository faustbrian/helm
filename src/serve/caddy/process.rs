use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use crate::output::{self, LogLevel, Persistence};

pub(super) fn ensure_caddy_installed() -> Result<()> {
    let output = match Command::new("caddy").arg("version").output() {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!(
                "caddy is not installed.\n\
                 install on macOS: brew install caddy\n\
                 install on Linux: https://caddyserver.com/docs/install"
            );
        }
        Err(error) => return Err(error).context("failed to execute caddy"),
    };

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::bail!("caddy is unavailable: {stderr}");
}

pub(super) fn reload_or_start_caddy(caddyfile_path: &Path) -> Result<()> {
    let config_path = caddyfile_path.to_string_lossy().into_owned();
    let reload_output = Command::new("caddy")
        .args(["reload", "--config", &config_path, "--adapter", "caddyfile"])
        .output()
        .context("failed to execute caddy reload")?;

    if reload_output.status.success() {
        output::event(
            "caddy",
            LogLevel::Success,
            &format!("Reloaded config {}", caddyfile_path.display()),
            Persistence::Persistent,
        );
        return Ok(());
    }

    let start_output = Command::new("caddy")
        .args(["start", "--config", &config_path, "--adapter", "caddyfile"])
        .output()
        .context("failed to execute caddy start")?;

    if start_output.status.success() {
        output::event(
            "caddy",
            LogLevel::Success,
            &format!("Started with config {}", caddyfile_path.display()),
            Persistence::Persistent,
        );
        return Ok(());
    }

    let reload_stderr = String::from_utf8_lossy(&reload_output.stderr);
    let start_stderr = String::from_utf8_lossy(&start_output.stderr);
    anyhow::bail!(
        "failed to reload or start caddy\nreload error: {reload_stderr}\nstart error: {start_stderr}"
    );
}

/// Attempts to trust local Caddy CA in system trust store.
///
/// # Errors
///
/// Returns an error when command execution fails.
pub fn trust_local_caddy_ca() -> Result<()> {
    let output = Command::new("caddy")
        .arg("trust")
        .output()
        .context("failed to execute caddy trust")?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    output::event(
        "caddy",
        LogLevel::Warn,
        &format!("Failed to trust local CA automatically: {stderr}"),
        Persistence::Persistent,
    );
    Ok(())
}
