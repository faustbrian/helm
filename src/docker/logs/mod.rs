//! docker logs module.
//!
//! Contains docker logs logic used by Helm command workflows.

use anyhow::{Context, Result};
use std::process::Command;

use crate::config::ServiceConfig;

use super::{is_dry_run, print_docker_command};
use args::build_logs_args;
use stream::stream_logs_with_prefix;

mod args;
mod many;
mod stream;

pub fn logs(
    service: &ServiceConfig,
    follow: bool,
    tail: Option<u64>,
    timestamps: bool,
) -> Result<()> {
    let container_name = service.container_name()?;
    let args = build_logs_args(&container_name, follow, tail, timestamps);

    if is_dry_run() {
        print_docker_command(&args);
        return Ok(());
    }

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let status = Command::new("docker")
        .args(&arg_refs)
        .status()
        .context("Failed to execute docker logs command")?;

    if !status.success() {
        anyhow::bail!("Failed to get logs for container '{container_name}'");
    }

    Ok(())
}

pub fn logs_prefixed(
    service: &ServiceConfig,
    follow: bool,
    tail: Option<u64>,
    timestamps: bool,
) -> Result<()> {
    let container_name = service.container_name()?;
    let args = build_logs_args(&container_name, follow, tail, timestamps);

    if is_dry_run() {
        print_docker_command(&args);
        return Ok(());
    }

    stream_logs_with_prefix(&args, &service.name, &container_name)
}

pub fn logs_many(
    services: &[ServiceConfig],
    follow: bool,
    tail: Option<u64>,
    timestamps: bool,
    prefix: bool,
) -> Result<()> {
    many::logs_many(services, follow, tail, timestamps, prefix)
}
