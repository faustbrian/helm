//! cli args commands daemon module.
//!
//! Contains cli args for `stackctl daemon` workflows.

mod adopt;
mod backup;
mod backups;
mod migration;
mod restore;
mod service;
mod trust;

use clap::{Args, Subcommand};
use std::path::PathBuf;

pub(crate) use adopt::DaemonAdoptArgs;
pub(crate) use backup::DaemonBackupArgs;
pub(crate) use backups::DaemonBackupsArgs;
pub(crate) use migration::{
    DaemonMigrationArgs, DaemonMigrationCommands, DaemonMigrationStatusArgs,
};
pub(crate) use restore::DaemonRestoreArgs;
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
    /// Create a verified recovery point for one project data service
    Backup(DaemonBackupArgs),
    /// List verified recovery points for one project
    Backups(DaemonBackupsArgs),
    /// Restore one verified recovery point to a reversible retained target
    Restore(DaemonRestoreArgs),
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
