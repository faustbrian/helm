//! docker manage container ops rm module.
//!
//! Contains docker manage container ops rm logic used by Helm command workflows.

use anyhow::{Context, Result};
use std::process::Command;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::super::super::{inspect_status, is_dry_run, print_docker_command};

/// Rms rm as part of the docker manage container ops rm workflow.
pub(super) fn rm(service: &ServiceConfig, force: bool) -> Result<()> {
    let container_name = service.container_name()?;

    if is_dry_run() {
        if force {
            print_docker_command(&["stop".to_owned(), container_name.clone()]);
        }
        print_docker_command(&["rm".to_owned(), container_name]);
        return Ok(());
    }

    if force {
        drop(
            Command::new("docker")
                .args(["stop", &container_name])
                .output(),
        );
    } else if let Some(status) = inspect_status(&container_name)
        && status == "running"
    {
        anyhow::bail!(
            "Container '{}' is still running. Stop it first with 'helm stop --service {}' or use --force",
            container_name,
            service.name
        );
    }

    let output = Command::new("docker")
        .args(["rm", &container_name])
        .output()
        .context("Failed to execute docker rm command")?;

    if output.status.success() {
        output::event(
            &service.name,
            LogLevel::Success,
            &format!("Removed container {container_name}"),
            Persistence::Persistent,
        );
    } else {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("Skipped removing container {container_name} because it does not exist"),
            Persistence::Persistent,
        );
    }

    Ok(())
}
