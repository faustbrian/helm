//! docker ops prune module.
//!
//! Contains docker container prune operation used by Helm command workflows.

use anyhow::{Context, Result};
use std::process::Command;

use crate::config::ServiceConfig;
use crate::docker::{
    LABEL_MANAGED, VALUE_MANAGED_TRUE, inspect_label, inspect_status, is_dry_run,
    print_docker_command,
};
use crate::output::{self, LogLevel, Persistence};

use super::common::run_docker_status;

pub(super) fn prune(force: bool, filters: &[String]) -> Result<()> {
    let mut args = vec!["container".to_owned(), "prune".to_owned()];
    if force {
        args.push("--force".to_owned());
    }
    for filter in filters {
        args.push("--filter".to_owned());
        args.push(filter.clone());
    }
    run_docker_status(&args, "Failed to execute docker container prune command")
}

pub(super) fn prune_stopped_container(service: &ServiceConfig) -> Result<()> {
    let container_name = service.container_name()?;
    if is_dry_run() {
        print_docker_command(&["rm".to_owned(), container_name]);
        return Ok(());
    }

    let Some(status) = inspect_status(&container_name) else {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("Skipped pruning container {container_name} because it was not found"),
            Persistence::Persistent,
        );
        return Ok(());
    };

    let Some(managed_label) = inspect_label(&container_name, LABEL_MANAGED) else {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("Skipped pruning container {container_name} because labels are unavailable"),
            Persistence::Persistent,
        );
        return Ok(());
    };
    if managed_label != VALUE_MANAGED_TRUE {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("Skipped pruning container {container_name} because it is not Helm-managed"),
            Persistence::Persistent,
        );
        return Ok(());
    }

    if is_active_status(&status) {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("Skipped pruning container {container_name} because it is {status}"),
            Persistence::Persistent,
        );
        return Ok(());
    }

    let rm_output = Command::new("docker")
        .args(["rm", &container_name])
        .output()
        .context("Failed to execute docker rm command while pruning")?;

    if rm_output.status.success() {
        output::event(
            &service.name,
            LogLevel::Success,
            &format!("Pruned stopped container {container_name}"),
            Persistence::Persistent,
        );
    } else {
        let stderr = String::from_utf8_lossy(&rm_output.stderr).trim().to_owned();
        anyhow::bail!(
            "Failed to prune container {container_name}: {}",
            if stderr.is_empty() {
                "unknown docker error"
            } else {
                &stderr
            }
        );
    }

    Ok(())
}

fn is_active_status(status: &str) -> bool {
    matches!(status, "running" | "paused" | "restarting")
}
