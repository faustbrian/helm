//! Shared docker up command helpers.

use anyhow::Result;
use std::process::Output;

use crate::docker::run_docker_output;

/// Runs `docker` with borrowed args and captures output.
pub(super) fn docker_output(args: &[&str], error_context: &str) -> Result<Output> {
    run_docker_output(args, error_context)
}

/// Runs `docker` with owned args and captures output.
pub(super) fn docker_output_owned(args: &[String], error_context: &str) -> Result<Output> {
    crate::docker::run_docker_output_owned(args, error_context)
}

/// Returns output on success, otherwise maps stderr into an anyhow error.
pub(super) fn ensure_success(output: Output, error_prefix: &str) -> Result<Output> {
    crate::docker::ensure_docker_output_success(output, error_prefix)
}
