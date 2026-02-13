//! Concrete `docker run`/reuse/remove lifecycle for serve containers.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::super::{mailhog_smtp_port, should_inject_frankenphp_server_name};
use args::build_run_args;
use dry_run::print_dry_run_container_start;
use existing::handle_existing_container;

mod args;
mod dry_run;
mod existing;
mod remove;

/// Ensures a serve container is running, creating or reusing as needed.
///
/// Behavior order:
/// - In dry-run mode, only logs planned actions.
/// - With `recreate`, forcibly removes existing container before run.
/// - Otherwise attempts reuse/start of existing container before creating a new one.
pub(super) fn ensure_container_running(
    target: &ServiceConfig,
    recreate: bool,
    runtime_image: &str,
    project_root: &Path,
    injected_env: &HashMap<String, String>,
) -> Result<()> {
    let container_name = target.container_name()?;

    if crate::docker::is_dry_run() {
        print_dry_run_container_start(
            target,
            &container_name,
            recreate,
            runtime_image,
            mailhog_smtp_port(target),
        );
        return Ok(());
    }

    if recreate {
        drop(
            Command::new("docker")
                .args(["rm", "-f", &container_name])
                .output(),
        );
    } else if handle_existing_container(&container_name)? {
        return Ok(());
    }

    let run_args = build_run_args(
        target,
        runtime_image,
        project_root,
        injected_env,
        should_inject_frankenphp_server_name(target, injected_env),
    )?;

    let run_output = Command::new("docker")
        .args(run_args.iter().map(String::as_str))
        .output()
        .context("failed to execute docker run for serve target")?;

    if !run_output.status.success() {
        let stderr = String::from_utf8_lossy(&run_output.stderr);
        anyhow::bail!("failed to run serve container: {stderr}");
    }

    output::event(
        &target.name,
        LogLevel::Success,
        "Started container for serve target",
        Persistence::Persistent,
    );
    Ok(())
}

/// Stops and removes the serve container for a target.
pub(super) fn remove_container(target: &ServiceConfig) -> Result<()> {
    remove::remove_container(target)
}
