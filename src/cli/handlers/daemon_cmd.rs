//! cli handlers daemon cmd module.
//!
//! Contains pre-config daemon command routing used by Helm command workflows.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::cli::args::{
    DaemonArgs, DaemonCommands, DaemonLogsArgs, DaemonStartArgs, DaemonStatusArgs, DaemonStopArgs,
};
use crate::config;
use crate::output::{self, LogLevel, Persistence};

pub(crate) fn handle_daemon(args: &DaemonArgs) -> Result<()> {
    match &args.command {
        DaemonCommands::Start(start) => handle_daemon_start(start),
        DaemonCommands::Status(status) => handle_daemon_status(status),
        DaemonCommands::Stop(stop) => handle_daemon_stop(stop),
        DaemonCommands::Logs(logs) => handle_daemon_logs(logs),
    }
}

fn handle_daemon_start(args: &DaemonStartArgs) -> Result<()> {
    let project_root = resolve_daemon_project_root(&args.path)?;
    output::event(
        "daemon",
        LogLevel::Info,
        &format!("Daemon start command routed for {}", project_root.display()),
        Persistence::Persistent,
    );
    Ok(())
}

fn handle_daemon_status(args: &DaemonStatusArgs) -> Result<()> {
    let project_root = resolve_daemon_project_root(&args.path)?;
    output::event(
        "daemon",
        LogLevel::Info,
        &format!("No daemon session recorded for {}", project_root.display()),
        Persistence::Persistent,
    );
    Ok(())
}

fn handle_daemon_stop(args: &DaemonStopArgs) -> Result<()> {
    let project_root = resolve_daemon_project_root(&args.path)?;
    output::event(
        "daemon",
        LogLevel::Info,
        &format!("No daemon session running for {}", project_root.display()),
        Persistence::Persistent,
    );
    Ok(())
}

fn handle_daemon_logs(args: &DaemonLogsArgs) -> Result<()> {
    let project_root = resolve_daemon_project_root(&args.path)?;
    output::event(
        "daemon",
        LogLevel::Info,
        &format!("No daemon logs found for {}", project_root.display()),
        Persistence::Persistent,
    );
    Ok(())
}

fn resolve_daemon_project_root(path: &Path) -> Result<PathBuf> {
    config::project_root_with(config::ProjectRootPathOptions::new(None, Some(path)))
}
