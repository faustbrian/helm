//! Runtime inspection helpers for serve app containers.

use anyhow::Result;

use super::docker_cmd::docker_output;
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
    let output = docker_output(
        &[
            "exec".to_owned(),
            container_name.clone(),
            "sh".to_owned(),
            "-lc".to_owned(),
            "tr '\\0' ' ' </proc/1/cmdline".to_owned(),
        ],
        "failed to inspect app runtime command line",
    )?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(
        String::from_utf8_lossy(&output.stdout).trim().to_owned(),
    ))
}
