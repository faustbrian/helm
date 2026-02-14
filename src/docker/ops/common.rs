//! docker ops common module.
//!
//! Shared Docker command execution helpers for docker ops.

use anyhow::{Context, Result};
use std::process::Command;

use crate::docker::{is_dry_run, print_docker_command};

pub(super) fn run_docker_status(args: &[String], context_message: &'static str) -> Result<()> {
    if is_dry_run() {
        print_docker_command(args);
        return Ok(());
    }

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let status = Command::new("docker")
        .args(&arg_refs)
        .status()
        .context(context_message)?;

    if !status.success() {
        anyhow::bail!("docker command failed: docker {}", args.join(" "));
    }

    Ok(())
}
