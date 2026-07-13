//! cli args commands daemon module.
//!
//! Contains cli args for `stackctl daemon` workflows.

mod service;

use clap::{Args, Subcommand};
use std::path::PathBuf;

pub(crate) use service::{
    DaemonServiceArgs, DaemonServiceCommands, DaemonServiceInstallArgs, DaemonServicePrintArgs,
};

#[derive(Args)]
pub(crate) struct DaemonArgs {
    #[command(subcommand)]
    pub(crate) command: DaemonCommands,
}

#[derive(Subcommand)]
pub(crate) enum DaemonCommands {
    /// Start a per-project Stackctl daemon
    Start(DaemonStartArgs),
    /// Run the authoritative singleton over watched project directories
    Watch(DaemonWatchArgs),
    /// Install or inspect a login-time daemon watch service
    Service(DaemonServiceArgs),
    /// Show daemon status for a project
    Status(DaemonStatusArgs),
    /// Stop a per-project Stackctl daemon
    Stop(DaemonStopArgs),
    /// Show daemon log output for a project
    Logs(DaemonLogsArgs),
    #[command(hide = true)]
    Run(DaemonRunArgs),
}

#[derive(Args)]
pub(crate) struct DaemonStartArgs {
    /// Project directory or nested path inside a Stackctl project
    #[arg(long, value_name = "DIR")]
    pub(crate) path: PathBuf,
}

#[derive(Args)]
pub(crate) struct DaemonStatusArgs {
    /// Project directory or nested path inside a Stackctl project
    #[arg(long, value_name = "DIR")]
    pub(crate) path: PathBuf,
}

#[derive(Args)]
pub(crate) struct DaemonWatchArgs {
    /// Authoritative parent directory to scan for Stackctl projects
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
    /// Project directory or nested path inside a Stackctl project
    #[arg(long, value_name = "DIR")]
    pub(crate) path: PathBuf,
}

#[derive(Args)]
pub(crate) struct DaemonLogsArgs {
    /// Project directory or nested path inside a Stackctl project
    #[arg(long, value_name = "DIR")]
    pub(crate) path: PathBuf,
}

#[derive(Args)]
pub(crate) struct DaemonRunArgs {
    /// Project directory or nested path inside a Stackctl project
    #[arg(long, value_name = "DIR")]
    pub(crate) path: PathBuf,
}
