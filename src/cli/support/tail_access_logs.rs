use anyhow::{Context, Result};
use std::process::Command;

use crate::output::{self, LogLevel, Persistence};
use crate::{docker, serve};

pub(crate) fn tail_access_logs(follow: bool, tail: Option<u64>) -> Result<()> {
    let path = serve::caddy_access_log_path()?;
    if !path.exists() {
        anyhow::bail!(
            "access log file not found at {}. start an app via `helm up` first",
            path.display()
        );
    }

    let mut args = vec!["-n".to_owned(), tail.unwrap_or(100).to_string()];
    if follow {
        args.push("-f".to_owned());
    }
    args.push(path.display().to_string());

    if docker::is_dry_run() {
        output::event(
            "logs",
            LogLevel::Info,
            &format!("[dry-run] tail {}", args.join(" ")),
            Persistence::Transient,
        );
        return Ok(());
    }

    let status = Command::new("tail")
        .args(args.iter().map(String::as_str))
        .status()
        .context("failed to tail access log file")?;
    if status.success() {
        return Ok(());
    }
    anyhow::bail!("tail command failed")
}
