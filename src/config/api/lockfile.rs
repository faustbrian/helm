//! config api lockfile module.
//!
//! Contains config api lockfile logic used by Helm command workflows.

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::super::{Config, LockedImage, Lockfile, paths};

pub struct LockfileDiff {
    pub missing: Vec<LockedImage>,
    pub changed: Vec<(LockedImage, LockedImage)>,
    pub extra: Vec<LockedImage>,
}

/// Builds image lock for command execution.
pub fn build_image_lock(config: &Config) -> Result<Lockfile> {
    let mut images: Vec<LockedImage> = config
        .service
        .iter()
        .map(|service| {
            Ok(LockedImage {
                service: service.name.clone(),
                image: service.image.clone(),
                resolved: resolve_image_digest(&service.image)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    images.sort_by(|left, right| left.service.cmp(&right.service));
    Ok(Lockfile { version: 1, images })
}

/// Loads lockfile with from persisted or external state.
pub fn load_lockfile_with(
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<Lockfile> {
    let path = paths::resolve_lockfile_path(config_path, project_root)?;
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))
}

/// Saves lockfile with to persisted or external state.
pub fn save_lockfile_with(
    lockfile: &Lockfile,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<PathBuf> {
    let path = paths::resolve_lockfile_path(config_path, project_root)?;
    let content =
        toml::to_string_pretty(lockfile).context("failed to serialize lockfile as TOML")?;
    std::fs::write(&path, content)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

pub fn lockfile_diff(expected: &Lockfile, actual: &Lockfile) -> LockfileDiff {
    let expected_map = by_service(expected);
    let actual_map = by_service(actual);

    let mut missing = Vec::new();
    let mut changed = Vec::new();
    let mut extra = Vec::new();

    for (service, expected_image) in &expected_map {
        match actual_map.get(service) {
            None => missing.push(expected_image.clone()),
            Some(actual_image)
                if actual_image.image != expected_image.image
                    || actual_image.resolved != expected_image.resolved =>
            {
                changed.push((expected_image.clone(), actual_image.clone()));
            }
            Some(_) => {}
        }
    }

    for (service, actual_image) in &actual_map {
        if !expected_map.contains_key(service) {
            extra.push(actual_image.clone());
        }
    }

    LockfileDiff {
        missing,
        changed,
        extra,
    }
}

/// Verifies lockfile with and reports actionable failures.
pub fn verify_lockfile_with(
    config: &Config,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<()> {
    let expected = build_image_lock(config)?;
    let actual = load_lockfile_with(config_path, project_root)
        .context("failed to load .helm.lock.toml; run `helm lock images` to generate it")?;
    let diff = lockfile_diff(&expected, &actual);
    if diff.missing.is_empty() && diff.changed.is_empty() && diff.extra.is_empty() {
        return Ok(());
    }

    anyhow::bail!("lockfile is out of sync; run `helm lock images`")
}

fn by_service(lockfile: &Lockfile) -> BTreeMap<String, LockedImage> {
    lockfile
        .images
        .iter()
        .map(|entry| (entry.service.clone(), entry.clone()))
        .collect()
}

/// Resolves image digest using configured inputs and runtime state.
fn resolve_image_digest(image: &str) -> Result<String> {
    if image.contains("@sha256:") {
        return Ok(image.to_owned());
    }

    if let Some(digest) = inspect_repo_digest(image)? {
        return Ok(digest);
    }

    let pull_status = Command::new("docker")
        .args(["pull", image])
        .status()
        .with_context(|| format!("failed to pull image '{image}' while resolving lockfile"))?;
    if !pull_status.success() {
        anyhow::bail!("failed to pull image '{image}' while resolving lockfile");
    }

    inspect_repo_digest(image)?
        .ok_or_else(|| anyhow::anyhow!("image '{image}' has no repo digest after pull"))
}

fn inspect_repo_digest(image: &str) -> Result<Option<String>> {
    let output = Command::new("docker")
        .args([
            "image",
            "inspect",
            "--format",
            "{{index .RepoDigests 0}}",
            image,
        ])
        .output()
        .with_context(|| format!("failed to inspect image '{image}'"))?;
    if !output.status.success() {
        return Ok(None);
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if value.is_empty() || value == "<no value>" || value == "<nil>" {
        return Ok(None);
    }
    Ok(Some(value))
}
