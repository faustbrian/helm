//! Shared docker command helpers for prune workflows.

use anyhow::Result;
use std::process::Output;

pub(super) fn docker_output(args: &[String], context_message: &str) -> Result<Output> {
    crate::docker::run_docker_output_owned(args, context_message)
}

pub(super) fn ensure_success(output: Output, error_prefix: &str) -> Result<Output> {
    if output.status.success() {
        return Ok(output);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    anyhow::bail!(
        "{error_prefix}: {}",
        if stderr.is_empty() {
            "unknown runtime error"
        } else {
            &stderr
        }
    );
}
