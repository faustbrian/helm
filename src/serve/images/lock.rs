//! Derived-image cache lock persistence and existence checks.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

use super::super::DerivedImageLock;

/// Returns the lockfile path used to map image signatures to derived tags.
fn derived_image_lock_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".config/helm/cache/derived-image-lock.toml"))
}

/// Reads derived image lock from persisted or external state.
pub(super) fn read_derived_image_lock() -> Result<DerivedImageLock> {
    let path = derived_image_lock_path()?;
    if !path.exists() {
        return Ok(DerivedImageLock::default());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let parsed: DerivedImageLock =
        toml::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(parsed)
}

/// Writes derived image lock to persisted or external state.
pub(super) fn write_derived_image_lock(lock: &DerivedImageLock) -> Result<()> {
    let path = derived_image_lock_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let content = toml::to_string_pretty(lock).context("failed to serialize derived image lock")?;
    std::fs::write(&path, content)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Returns whether a docker image tag currently exists locally.
pub(super) fn docker_image_exists(tag: &str) -> Result<bool> {
    if crate::docker::is_dry_run() {
        return Ok(false);
    }
    let output = Command::new("docker")
        .args(["image", "inspect", tag])
        .output()
        .context("failed to inspect docker image")?;
    Ok(output.status.success())
}
