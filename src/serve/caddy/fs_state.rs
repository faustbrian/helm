use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use super::CaddyState;

pub(super) fn caddy_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".config/helm/caddy"))
}

/// Returns the Caddy access log path used by Helm.
///
/// # Errors
///
/// Returns an error if HOME is not set.
pub fn caddy_access_log_path() -> Result<PathBuf> {
    Ok(caddy_dir()?.join("access.log"))
}

pub(super) fn read_caddy_state(path: &Path) -> Result<CaddyState> {
    if !path.exists() {
        return Ok(CaddyState::default());
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let state: CaddyState =
        toml::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(state)
}

pub(super) fn write_caddy_state_and_file(
    state_path: &Path,
    caddyfile_path: &Path,
    state: &CaddyState,
    caddyfile: &str,
) -> Result<()> {
    let state_content = toml::to_string_pretty(state).context("failed to serialize caddy state")?;
    std::fs::write(state_path, state_content)
        .with_context(|| format!("failed to write {}", state_path.display()))?;
    std::fs::write(caddyfile_path, caddyfile)
        .with_context(|| format!("failed to write {}", caddyfile_path.display()))?;
    Ok(())
}
