//! One-off container execution for non-running serve targets.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::super::{resolve_runtime_image, resolve_volume_mapping};

/// Runs a command in a temporary container derived from target runtime settings.
///
/// This mirrors target volume/env wiring so commands behave like in the regular
/// app container.
pub(super) fn run_command_one_off(
    target: &ServiceConfig,
    command: &[String],
    tty: bool,
    project_root: &Path,
    injected_env: &HashMap<String, String>,
) -> Result<()> {
    let runtime_image = resolve_runtime_image(target, true, injected_env)?;
    let mut args = vec!["run".to_owned(), "--rm".to_owned()];
    args.push(if tty { "-it" } else { "-i" }.to_owned());
    args.push("-w".to_owned());
    args.push("/app".to_owned());

    if let Some(volumes) = &target.volumes {
        for volume in volumes {
            args.push("-v".to_owned());
            args.push(resolve_volume_mapping(volume, project_root)?);
        }
    }

    for (key, value) in injected_env {
        args.push("-e".to_owned());
        args.push(format!("{key}={value}"));
    }

    if let Some(env_vars) = &target.env {
        for (key, value) in env_vars {
            args.push("-e".to_owned());
            args.push(format!("{key}={value}"));
        }
    }

    args.push(runtime_image);
    args.extend(command.iter().cloned());

    if crate::docker::is_dry_run() {
        output::event(
            &target.name,
            LogLevel::Info,
            &format!("[dry-run] docker {}", args.join(" ")),
            Persistence::Transient,
        );
        return Ok(());
    }

    let status = Command::new("docker")
        .args(args.iter().map(String::as_str))
        .status()
        .context("failed to execute one-off app command")?;

    if !status.success() {
        anyhow::bail!("one-off app command failed");
    }

    Ok(())
}
