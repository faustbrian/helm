//! Shared docker command helpers for lifecycle operations.

use anyhow::Result;
use std::process::Output;

/// Best-effort forced container removal.
pub(super) fn force_remove_container(container_name: &str) {
    drop(docker_output(
        &["rm", "-f", container_name],
        "failed to remove container",
    ));
}

pub(super) fn docker_output(args: &[&str], context: &str) -> Result<Output> {
    crate::docker::run_docker_output(args, context)
}

pub(super) fn checked_output(args: &[&str], context: &str, error_prefix: &str) -> Result<()> {
    let output = docker_output(args, context)?;
    crate::docker::ensure_docker_output_success(output, error_prefix)?;
    Ok(())
}
