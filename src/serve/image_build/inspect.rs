//! Base-image inspection used by derived image planning.

use anyhow::Result;
use std::collections::HashSet;

use super::docker_cmd::docker_output;

/// Returns the set of PHP modules reported by `php -m` inside `base_image`.
pub(super) fn installed_php_modules(base_image: &str) -> Result<HashSet<String>> {
    if crate::docker::is_dry_run() {
        return Ok(HashSet::new());
    }

    let output = docker_output(
        &["run", "--rm", base_image, "php", "-m"],
        &format!("failed to inspect preinstalled PHP modules in {base_image}"),
    )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("failed to inspect preinstalled PHP modules in '{base_image}': {stderr}");
    }

    let modules = String::from_utf8_lossy(&output.stdout);
    Ok(modules
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('['))
        .map(str::to_lowercase)
        .collect())
}
