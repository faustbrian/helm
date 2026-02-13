//! Container stop/remove steps for serve lifecycle teardown.

use anyhow::Result;
use std::process::Command;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

/// Stops then removes the target serve container.
pub(super) fn remove_container(target: &ServiceConfig) -> Result<()> {
    let container_name = target.container_name()?;

    if crate::docker::is_dry_run() {
        output::event(
            &target.name,
            LogLevel::Info,
            &format!("[dry-run] docker stop {container_name}"),
            Persistence::Transient,
        );
        output::event(
            &target.name,
            LogLevel::Info,
            &format!("[dry-run] docker rm {container_name}"),
            Persistence::Transient,
        );
        return Ok(());
    }

    drop(
        Command::new("docker")
            .args(["stop", &container_name])
            .output(),
    );
    drop(
        Command::new("docker")
            .args(["rm", &container_name])
            .output(),
    );
    output::event(
        &target.name,
        LogLevel::Success,
        &format!("Stopped and removed serve container {container_name}"),
        Persistence::Persistent,
    );
    Ok(())
}
