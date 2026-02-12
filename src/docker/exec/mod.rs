use anyhow::{Context, Result};
use std::process::{Child, Command, Stdio};

use crate::config::ServiceConfig;

use super::{is_dry_run, print_docker_command};
use args::{build_exec_args, interactive_client_args, piped_client_args};

mod args;

pub fn exec_interactive(service: &ServiceConfig, tty: bool) -> Result<()> {
    let container_name = service.container_name()?;
    let client_args = interactive_client_args(service);
    let args = build_exec_args(&container_name, &client_args, tty);

    if is_dry_run() {
        print_docker_command(&args);
        return Ok(());
    }

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let status = Command::new("docker")
        .args(&arg_refs)
        .status()
        .context("Failed to execute docker exec command")?;

    if !status.success() {
        anyhow::bail!("Interactive session failed");
    }

    Ok(())
}

pub fn exec_piped(service: &ServiceConfig, tty: bool) -> Result<Child> {
    let container_name = service.container_name()?;
    let client_args = piped_client_args(service)?;
    let args = build_exec_args(&container_name, &client_args, tty);

    if is_dry_run() {
        print_docker_command(&args);
        return Command::new("cat")
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn dry-run process");
    }

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    Command::new("docker")
        .args(&arg_refs)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn docker exec command")
}

pub fn exec_command(service: &ServiceConfig, command: &[String], tty: bool) -> Result<()> {
    let container_name = service.container_name()?;
    let args = build_exec_args(&container_name, command, tty);

    if is_dry_run() {
        print_docker_command(&args);
        return Ok(());
    }

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let status = Command::new("docker")
        .args(&arg_refs)
        .status()
        .context("Failed to execute command in container")?;

    if !status.success() {
        anyhow::bail!("Command failed in container '{container_name}'");
    }

    Ok(())
}
