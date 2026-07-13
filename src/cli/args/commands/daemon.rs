//! cli args commands daemon module.
//!
//! Contains cli args for `stackctl daemon` workflows.

mod adopt;
mod migration;
mod service;
mod trust;

use clap::{Args, Subcommand};
use std::path::PathBuf;

pub(crate) use adopt::DaemonAdoptArgs;
pub(crate) use migration::{
    DaemonMigrationArgs, DaemonMigrationCommands, DaemonMigrationStatusArgs,
};
pub(crate) use service::{
    DaemonServiceArgs, DaemonServiceCommands, DaemonServiceInstallArgs, DaemonServicePrintArgs,
};
pub(crate) use trust::{DaemonTrustArgs, DaemonTrustCommands};

#[derive(Args)]
pub(crate) struct DaemonArgs {
    #[command(subcommand)]
    pub(crate) command: DaemonCommands,
}

#[derive(Subcommand)]
pub(crate) enum DaemonCommands {
    /// Run the authoritative singleton over watched project directories
    Watch(DaemonWatchArgs),
    /// Install or inspect a login-time daemon watch service
    Service(DaemonServiceArgs),
    /// Verify that the per-user singleton is responsive
    Status,
    /// Request one immediate complete watched-root reconciliation
    Reconcile,
    /// Explicitly reactivate the exact retained state for one project
    Adopt(DaemonAdoptArgs),
    /// Inspect reversible resource migrations
    Migration(DaemonMigrationArgs),
    /// Manage trust for the singleton Stackctl certificate authority
    Trust(DaemonTrustArgs),
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
