//! docker ops port module.
//!
//! Contains docker port operation used by Helm command workflows.

use anyhow::{Context, Result};
use std::process::Command;

use crate::config::ServiceConfig;
use crate::docker::is_dry_run;

use super::common::run_docker_status;

pub(super) fn port(service: &ServiceConfig, private_port: Option<&str>) -> Result<()> {
    let container_name = service.container_name()?;
    let mut args = vec!["port".to_owned(), container_name];
    if let Some(port_name) = private_port {
        args.push(port_name.to_owned());
    }
    run_docker_status(&args, "Failed to execute docker port command")
}

pub(super) fn port_output(service: &ServiceConfig, private_port: Option<&str>) -> Result<String> {
    let container_name = service.container_name()?;
    let mut args = vec!["port".to_owned(), container_name];
    if let Some(port_name) = private_port {
        args.push(port_name.to_owned());
    }
    if is_dry_run() {
        return Ok(String::new());
    }

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = Command::new("docker")
        .args(&arg_refs)
        .output()
        .context("Failed to execute docker port command")?;
    if !output.status.success() {
        anyhow::bail!("docker command failed: docker {}", args.join(" "));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
