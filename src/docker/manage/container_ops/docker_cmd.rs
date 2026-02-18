//! Shared docker container-op command helpers.

use anyhow::Result;
use std::process::Output;

/// Runs `docker` with args and captures output.
pub(super) fn docker_output(args: &[&str], error_context: &str) -> Result<Output> {
    crate::docker::run_docker_output(args, error_context)
}

/// Best-effort docker command used in cleanup paths.
pub(super) fn try_docker_output(args: &[&str]) {
    drop(crate::docker::run_docker_output(
        args,
        "failed to execute docker cleanup command",
    ));
}
