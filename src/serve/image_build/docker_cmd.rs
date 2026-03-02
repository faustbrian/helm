//! Shared docker command helpers for serve image build flows.

use anyhow::Result;
use std::process::Output;

pub(super) fn docker_status(args: &[&str], context: &str) -> Result<std::process::ExitStatus> {
    crate::docker::run_docker_status(args, context)
}

pub(super) fn docker_output(args: &[&str], context: &str) -> Result<Output> {
    crate::docker::run_docker_output(args, context)
}
