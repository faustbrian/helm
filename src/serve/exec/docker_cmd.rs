//! Shared docker command execution helpers for serve exec flows.

use anyhow::Result;
use std::process::Output;

use crate::output::{self, LogLevel, Persistence};

pub(super) fn run_or_log_docker(
    target_name: &str,
    args: &[String],
    execution_context: &str,
    failed_message: &str,
) -> Result<()> {
    if crate::docker::is_dry_run() {
        output::event(
            target_name,
            LogLevel::Info,
            &format!("[dry-run] {}", crate::docker::runtime_command_text(args)),
            Persistence::Transient,
        );
        return Ok(());
    }

    let status = crate::docker::run_docker_status_owned(args, execution_context)?;
    if !status.success() {
        anyhow::bail!("{failed_message}");
    }
    Ok(())
}

pub(super) fn docker_output(args: &[String], execution_context: &str) -> Result<Output> {
    crate::docker::run_docker_output_owned(args, execution_context)
}
