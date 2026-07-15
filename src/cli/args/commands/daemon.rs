//! cli args commands daemon module.
//!
//! Contains cli args for `stackctl daemon` workflows.

mod adopt;
mod backup;
mod backups;
mod benchmark;
mod benchmark_evidence_scenario;
mod migration;
mod prune;
mod restore;
mod retained;
mod service;
mod trust;

use clap::{Args, Subcommand};
use std::path::PathBuf;

pub(crate) use adopt::DaemonAdoptArgs;
pub(crate) use backup::DaemonBackupArgs;
pub(crate) use backups::DaemonBackupsArgs;
pub(crate) use benchmark::DaemonBenchmarkArgs;
pub(crate) use benchmark_evidence_scenario::BenchmarkEvidenceScenario;
pub(crate) use migration::{
    DaemonMigrationArgs, DaemonMigrationCommands, DaemonMigrationDecisionArgs,
    DaemonMigrationStatusArgs,
};
pub(crate) use prune::{
    DaemonPruneArgs, DaemonPruneCommands, DaemonPruneExecuteArgs, DaemonPrunePlanArgs,
};
pub(crate) use restore::DaemonRestoreArgs;
pub(crate) use retained::DaemonRetainedArgs;
pub(crate) use service::{
    DaemonServiceArgs, DaemonServiceCommands, DaemonServiceInstallArgs, DaemonServicePrintArgs,
    DaemonServiceUninstallArgs,
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
    /// Show retained resources whose project configs are no longer registered
    Retained(DaemonRetainedArgs),
    /// Request one immediate complete watched-root reconciliation
    Reconcile,
    /// Emit one read-only JSON snapshot of owned Engine resource usage
    Benchmark(DaemonBenchmarkArgs),
    /// Explicitly reactivate the exact retained state for one project
    Adopt(DaemonAdoptArgs),
    /// Create a verified recovery point for one project data service
    Backup(DaemonBackupArgs),
    /// List verified recovery points for one project
    Backups(DaemonBackupsArgs),
    /// Restore one verified recovery point to a reversible retained target
    Restore(DaemonRestoreArgs),
    /// Plan explicit destructive cleanup of retained project data
    Prune(DaemonPruneArgs),
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
