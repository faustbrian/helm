use anyhow::{Context, Result};
use std::process::Command;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::{UpOptions, inspect_status, is_dry_run};

mod args_builder;
mod dry_run;
mod state;

/// Ensures the service container is running.
pub fn up(service: &ServiceConfig, options: UpOptions) -> Result<()> {
    let container_name = service.container_name()?;

    if is_dry_run() {
        return dry_run::describe(service, options, &container_name);
    }

    if state::ensure_or_start_existing(&container_name, options.recreate)? {
        return Ok(());
    }

    state::ensure_image_available(service, options.pull)?;

    let run_args = args_builder::build_run_args(service, &container_name);
    let args_refs: Vec<&str> = run_args.iter().map(String::as_str).collect();
    let output = Command::new("docker")
        .args(&args_refs)
        .output()
        .context("Failed to execute docker run command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to start container: {stderr}");
    }

    output::event(
        &service.name,
        LogLevel::Success,
        &format!("Started container {container_name}"),
        Persistence::Persistent,
    );
    Ok(())
}

pub(super) fn inspect_image_exists(image: &str) -> Result<bool> {
    let inspect = Command::new("docker")
        .args(["image", "inspect", image])
        .output()
        .context("Failed to inspect docker image")?;
    Ok(inspect.status.success())
}

pub(super) fn remove_container(container_name: &str) {
    drop(
        Command::new("docker")
            .args(["rm", "-f", container_name])
            .output(),
    );
}

pub(super) fn start_container(container_name: &str) -> Result<bool> {
    if let Some(status) = inspect_status(container_name) {
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
                .context("Failed to execute docker start command")?;

            if !start_output.status.success() {
                let stderr = String::from_utf8_lossy(&start_output.stderr);
                anyhow::bail!("Failed to start container: {stderr}");
            }

            output::event(
                container_name,
                LogLevel::Success,
                "Started existing container",
                Persistence::Persistent,
            );
            return Ok(true);
        }

        remove_container(container_name);
    }

    Ok(false)
}
