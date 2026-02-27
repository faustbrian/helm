//! docker exec module.
//!
//! Contains docker exec logic used by Helm command workflows.

use anyhow::Result;
use std::process::Child;

use crate::config::ServiceConfig;

use super::{is_dry_run, print_docker_command};
pub(crate) use args::build_exec_args;
use args::{interactive_client_args, piped_client_args};
use run::{dry_run_process, run_docker_status, spawn_docker_piped};

mod args;
mod run;

/// Execs interactive as part of the docker exec workflow.
pub fn exec_interactive(service: &ServiceConfig, tty: bool) -> Result<()> {
    let container_name = service.container_name()?;
    let client_args = interactive_client_args(service);
    let args = build_exec_args(&container_name, &client_args, tty);

    if is_dry_run() {
        print_docker_command(&args);
        return Ok(());
    }

    let status = run_docker_status(&args, &super::runtime_command_error_context("exec"))?;

    if !status.success() {
        anyhow::bail!("Interactive session failed");
    }

    Ok(())
}

/// Execs piped as part of the docker exec workflow.
pub fn exec_piped(service: &ServiceConfig, tty: bool) -> Result<Child> {
    let container_name = service.container_name()?;
    let client_args = piped_client_args(service)?;
    let args = build_exec_args(&container_name, &client_args, tty);

    if is_dry_run() {
        print_docker_command(&args);
        return dry_run_process();
    }

    spawn_docker_piped(&args)
}

/// Execs command as part of the docker exec workflow.
pub fn exec_command(service: &ServiceConfig, command: &[String], tty: bool) -> Result<()> {
    let container_name = service.container_name()?;
    let args = build_exec_args(&container_name, command, tty);

    if is_dry_run() {
        print_docker_command(&args);
        return Ok(());
    }

    let status = run_docker_status(&args, "Failed to execute command in container")?;

    if !status.success() {
        anyhow::bail!("Command failed in container '{container_name}'");
    }

    Ok(())
}
