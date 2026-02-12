use anyhow::{Context, Result};
use std::process::Command;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::super::super::{inspect_status, is_dry_run, print_docker_command};

pub(super) fn restart(service: &ServiceConfig) -> Result<()> {
    let container_name = service.container_name()?;

    if is_dry_run() {
        print_docker_command(&["restart".to_owned(), container_name]);
        return Ok(());
    }

    if inspect_status(&container_name).is_none() {
        anyhow::bail!("Container {container_name} does not exist");
    }

    let output = Command::new("docker")
        .args(["restart", &container_name])
        .output()
        .context("Failed to execute docker restart command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to restart container: {stderr}");
    }

    output::event(
        &service.name,
        LogLevel::Success,
        &format!("Restarted container {container_name}"),
        Persistence::Persistent,
    );
    Ok(())
}
