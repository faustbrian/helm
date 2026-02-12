use anyhow::{Context, Result};
use std::process::Command;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

/// Executes a command inside the app container.
///
/// # Errors
///
/// Returns an error if command execution fails.
pub(super) fn exec_command(target: &ServiceConfig, command: &[String], tty: bool) -> Result<()> {
    if command.is_empty() {
        anyhow::bail!("no command specified");
    }

    let container_name = target.container_name()?;
    let args = build_exec_args(&container_name, command, tty);

    if crate::docker::is_dry_run() {
        output::event(
            &target.name,
            LogLevel::Info,
            &format!("[dry-run] docker {}", args.join(" ")),
            Persistence::Transient,
        );
        return Ok(());
    }

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let status = Command::new("docker")
        .args(&arg_refs)
        .status()
        .context("failed to execute artisan command in serve container")?;

    if !status.success() {
        anyhow::bail!("command failed in container '{container_name}'");
    }

    Ok(())
}

pub(crate) fn build_exec_args(container_name: &str, command: &[String], tty: bool) -> Vec<String> {
    let mut args = vec![
        "exec".to_owned(),
        if tty { "-it" } else { "-i" }.to_owned(),
        container_name.to_owned(),
    ];
    args.extend(command.iter().cloned());
    args
}
