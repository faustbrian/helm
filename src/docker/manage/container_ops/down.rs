use anyhow::{Context, Result};
use std::process::Command;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::super::super::{is_dry_run, print_docker_command};

pub(super) fn down(service: &ServiceConfig) -> Result<()> {
    let container_name = service.container_name()?;

    if is_dry_run() {
        print_docker_command(&["stop".to_owned(), container_name.clone()]);
        print_docker_command(&["rm".to_owned(), container_name]);
        return Ok(());
    }

    let stop_output = Command::new("docker")
        .args(["stop", &container_name])
        .output()
        .context("Failed to execute docker stop command")?;

    if stop_output.status.success() {
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

    let rm_output = Command::new("docker")
        .args(["rm", &container_name])
        .output()
        .context("Failed to execute docker rm command")?;

    if rm_output.status.success() {
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
            &format!("Skipped removing container {container_name} because it was not found"),
            Persistence::Persistent,
        );
    }

    Ok(())
}
