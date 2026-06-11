//! cli args commands daemon module.
//!
//! Contains cli args for `helm daemon` workflows.

use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Args)]
pub(crate) struct DaemonArgs {
    #[command(subcommand)]
    pub(crate) command: DaemonCommands,
}

#[derive(Subcommand)]
pub(crate) enum DaemonCommands {
    /// Start a per-project Helm daemon
    Start(DaemonStartArgs),
    /// Watch directories for Helm projects and ensure daemons are running
    Watch(DaemonWatchArgs),
    /// Show daemon status for a project
    Status(DaemonStatusArgs),
    /// Stop a per-project Helm daemon
    Stop(DaemonStopArgs),
    /// Show daemon log output for a project
    Logs(DaemonLogsArgs),
    #[command(hide = true)]
    Run(DaemonRunArgs),
}

#[derive(Args)]
pub(crate) struct DaemonStartArgs {
    /// Project directory or nested path inside a Helm project
    #[arg(long, value_name = "DIR")]
    pub(crate) path: PathBuf,
}

#[derive(Args)]
pub(crate) struct DaemonStatusArgs {
    /// Project directory or nested path inside a Helm project
    #[arg(long, value_name = "DIR")]
    pub(crate) path: PathBuf,
}

#[derive(Args)]
pub(crate) struct DaemonWatchArgs {
    /// Directory to scan for Helm projects
    #[arg(long, value_name = "DIR", required = true)]
    pub(crate) dir: Vec<PathBuf>,
    /// Run one discovery pass and exit
    #[arg(long, default_value_t = false)]
    pub(crate) once: bool,
    /// Seconds between discovery scans
    #[arg(long, default_value_t = 30)]
    pub(crate) interval: u64,
}

#[derive(Args)]
pub(crate) struct DaemonStopArgs {
    /// Project directory or nested path inside a Helm project
    #[arg(long, value_name = "DIR")]
    pub(crate) path: PathBuf,
}

#[derive(Args)]
pub(crate) struct DaemonLogsArgs {
    /// Project directory or nested path inside a Helm project
    #[arg(long, value_name = "DIR")]
    pub(crate) path: PathBuf,
}

#[derive(Args)]
pub(crate) struct DaemonRunArgs {
    /// Project directory or nested path inside a Helm project
    #[arg(long, value_name = "DIR")]
    pub(crate) path: PathBuf,
}
