//! Shared docker logs process runners.

use anyhow::Result;
use std::process::{Child, ExitStatus};

pub(super) fn run_docker_status(args: &[String], error_context: &str) -> Result<ExitStatus> {
    crate::docker::run_docker_status_owned(args, error_context)
}

pub(super) fn spawn_docker_stream(args: &[String], error_context: &str) -> Result<Child> {
    let arg_refs = crate::docker::docker_arg_refs(args);
    crate::docker::spawn_docker_stdout_stderr_piped(&arg_refs, error_context)
}
