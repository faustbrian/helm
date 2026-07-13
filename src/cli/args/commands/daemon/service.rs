//! cli args for `stackctl daemon service` workflows.

use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Args)]
pub(crate) struct DaemonServiceArgs {
    #[command(subcommand)]
    pub(crate) command: DaemonServiceCommands,
}

#[derive(Subcommand)]
pub(crate) enum DaemonServiceCommands {
    /// Install and start the login-time singleton control plane
    Install(DaemonServiceInstallArgs),
    /// Show service installation state
    Status,
    /// Print the rendered service definition without installing it
    Print(DaemonServicePrintArgs),
    /// Stop and remove the installed service definition
    Uninstall,
}

#[derive(Args)]
pub(crate) struct DaemonServiceInstallArgs {
    /// Authoritative parent directory to scan for Stackctl projects
    #[arg(long, value_name = "DIR", required = true)]
    pub(crate) dir: Vec<PathBuf>,
    /// Seconds between discovery scans
    #[arg(long, default_value_t = 30)]
    pub(crate) interval: u64,
}

#[derive(Args)]
pub(crate) struct DaemonServicePrintArgs {
    /// Authoritative parent directory to scan for Stackctl projects
    #[arg(long, value_name = "DIR", required = true)]
    pub(crate) dir: Vec<PathBuf>,
    /// Seconds between discovery scans
    #[arg(long, default_value_t = 30)]
    pub(crate) interval: u64,
}
