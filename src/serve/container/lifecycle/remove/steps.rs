//! Docker step execution helpers for serve container removal.

use anyhow::Result;

use super::super::docker_cmd::docker_output;

#[derive(Clone, Copy)]
pub(super) enum StepOutcome {
    Performed,
    Missing,
}

pub(super) fn run_docker_step(
    args: [&str; 2],
    action: &str,
    container_name: &str,
) -> Result<StepOutcome> {
    let output = docker_output(&args, &format!("failed to execute docker {action}"))?;

    if output.status.success() {
        return Ok(StepOutcome::Performed);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.to_ascii_lowercase().contains("no such container") {
        return Ok(StepOutcome::Missing);
    }

    anyhow::bail!("failed to {action} serve container '{container_name}': {stderr}");
}
