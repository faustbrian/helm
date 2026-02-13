//! PHP extension verification for resolved serve runtime images.

use anyhow::{Context, Result};

use crate::config::ServiceConfig;

/// Verifies configured PHP extensions against modules available in runtime image.
///
/// Returns `Ok(None)` when no extensions are configured for the target.
pub(crate) fn verify_php_extensions(
    target: &ServiceConfig,
) -> Result<Option<super::PhpExtensionCheck>> {
    let Some(extensions) = target
        .php_extensions
        .as_ref()
        .filter(|exts| !exts.is_empty())
        .map(|exts| super::normalize_php_extensions(exts))
    else {
        return Ok(None);
    };

    let runtime_image =
        super::resolve_runtime_image(target, true, &std::collections::HashMap::new())?;
    if crate::docker::is_dry_run() {
        return Ok(Some(super::PhpExtensionCheck {
            target: target.name.clone(),
            image: runtime_image,
            missing: Vec::new(),
        }));
    }

    let output = std::process::Command::new("docker")
        .args(["run", "--rm", &runtime_image, "php", "-m"])
        .output()
        .context("failed to execute php -m for serve extension verification")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("failed to inspect PHP extensions for '{runtime_image}': {stderr}");
    }

    let modules = String::from_utf8_lossy(&output.stdout).to_lowercase();
    let missing: Vec<String> = extensions
        .iter()
        .filter(|ext| !modules.contains(&ext.to_lowercase()))
        .cloned()
        .collect();

    Ok(Some(super::PhpExtensionCheck {
        target: target.name.clone(),
        image: runtime_image,
        missing,
    }))
}
