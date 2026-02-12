use anyhow::{Context, Result};
use std::process::Command;

use crate::output::{self, LogLevel, Persistence};

pub(super) fn handle_existing_container(container_name: &str) -> Result<bool> {
    let Some(status) = crate::docker::inspect_status(container_name) else {
        return Ok(false);
    };

    if status == "running" {
        output::event(
            container_name,
            LogLevel::Info,
            "Container already running",
            Persistence::Persistent,
        );
        return Ok(true);
    }

    if status == "exited" || status == "created" {
        let start_output = Command::new("docker")
            .args(["start", container_name])
            .output()
            .context("failed to execute docker start")?;

        if !start_output.status.success() {
            let stderr = String::from_utf8_lossy(&start_output.stderr);
            anyhow::bail!("failed to start existing serve container: {stderr}");
        }

        output::event(
            container_name,
            LogLevel::Success,
            "Started existing container",
            Persistence::Persistent,
        );
        return Ok(true);
    }

    drop(
        Command::new("docker")
            .args(["rm", "-f", container_name])
            .output(),
    );
    Ok(false)
}
