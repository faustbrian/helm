//! Docker build execution for derived serve images.

use anyhow::{Context, Result};
use std::process::Command;

use crate::output::{self, LogLevel, Persistence};

/// Builds a derived image from in-memory Dockerfile content.
///
/// Writes build context into a temporary directory, runs `docker build`, then
/// removes the temporary directory.
pub(super) fn build_derived_image(tag: &str, dockerfile: &str) -> Result<()> {
    let dir = std::env::temp_dir().join(format!(
        "helm-serve-build-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let dockerfile_path = dir.join("Dockerfile");
    std::fs::write(&dockerfile_path, dockerfile)
        .with_context(|| format!("failed to write {}", dockerfile_path.display()))?;

    output::event(
        "build",
        LogLevel::Info,
        &format!("Building derived serve image {tag}"),
        Persistence::Persistent,
    );
    let status = Command::new("docker")
        .args(["build", "-t", tag, "-f"])
        .arg(&dockerfile_path)
        .arg(&dir)
        .status()
        .context("failed to execute docker build for serve image")?;

    drop(std::fs::remove_dir_all(&dir));

    if status.success() {
        return Ok(());
    }

    anyhow::bail!("failed to build derived serve image {tag}");
}
