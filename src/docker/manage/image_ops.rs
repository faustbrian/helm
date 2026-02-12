use anyhow::{Context, Result};
use std::process::Command;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::super::{is_dry_run, print_docker_command};

pub(super) fn pull(service: &ServiceConfig) -> Result<()> {
    output::event(
        &service.name,
        LogLevel::Info,
        &format!("Pulling image {}", service.image),
        Persistence::Persistent,
    );

    if is_dry_run() {
        print_docker_command(&["pull".to_owned(), service.image.clone()]);
        return Ok(());
    }

    let status = Command::new("docker")
        .args(["pull", &service.image])
        .status()
        .context("Failed to execute docker pull command")?;

    if !status.success() {
        anyhow::bail!("Failed to pull image {}", service.image);
    }

    output::event(
        &service.name,
        LogLevel::Success,
        &format!("Pulled image {}", service.image),
        Persistence::Persistent,
    );
    Ok(())
}
