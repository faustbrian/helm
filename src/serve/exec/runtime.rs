use anyhow::{Context, Result};
use std::process::Command;

use crate::config::ServiceConfig;

/// Returns PID1 command line for a running app container.
///
/// # Errors
///
/// Returns an error when docker exec invocation fails unexpectedly.
pub(crate) fn runtime_cmdline(target: &ServiceConfig) -> Result<Option<String>> {
    let container_name = target.container_name()?;
    if crate::docker::inspect_status(&container_name).as_deref() != Some("running") {
        return Ok(None);
    }
    let output = Command::new("docker")
        .args([
            "exec",
            &container_name,
            "sh",
            "-lc",
            "tr '\\0' ' ' </proc/1/cmdline",
        ])
        .output()
        .context("failed to inspect app runtime command line")?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(
        String::from_utf8_lossy(&output.stdout).trim().to_owned(),
    ))
}
