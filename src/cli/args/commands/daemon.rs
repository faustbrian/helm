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
    /// Show daemon status for a project
    Status(DaemonStatusArgs),
    /// Stop a per-project Helm daemon
    Stop(DaemonStopArgs),
    /// Show daemon log output for a project
    Logs(DaemonLogsArgs),
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
