//! docker manage container ops stop module.
//!
//! Contains docker manage container ops stop logic used by Helm command workflows.

use anyhow::{Context, Result};
use std::process::Command;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::super::super::{is_dry_run, print_docker_command};

/// Stops stop as part of the docker manage container ops stop workflow.
pub(super) fn stop(service: &ServiceConfig) -> Result<()> {
    let container_name = service.container_name()?;

    if is_dry_run() {
        print_docker_command(&["stop".to_owned(), container_name]);
        return Ok(());
    }

    let output = Command::new("docker")
        .args(["stop", &container_name])
        .output()
        .context("Failed to execute docker stop command")?;

    if output.status.success() {
        output::event(
            &service.name,
            LogLevel::Success,
            &format!("Stopped container {container_name}"),
            Persistence::Persistent,
        );
    } else {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("Skipped stopping container {container_name} because it is not running"),
            Persistence::Persistent,
        );
    }

    Ok(())
}
