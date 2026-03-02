//! Shared command execution helpers for trust workflows.

use anyhow::{Context, Result};
use std::process::{Command, Output};

pub(super) fn run_output(command: &mut Command, context: &str) -> Result<Output> {
    command.output().with_context(|| context.to_owned())
}

pub(super) fn checked_output(
    command: &mut Command,
    context: &str,
    error_prefix: &str,
) -> Result<Output> {
    let output = run_output(command, context)?;
    ensure_output_success(output, error_prefix)
}

pub(super) fn ensure_output_success(output: Output, error_prefix: &str) -> Result<Output> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{error_prefix}: {stderr}");
    }
    Ok(output)
}
