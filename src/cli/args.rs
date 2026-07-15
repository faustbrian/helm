//! cli args module.
//!
//! Contains cli args logic used by Stackctl command workflows.

use clap::Parser;
use std::path::Path;
use std::path::PathBuf;

mod commands;
mod config_commands;
mod env_commands;
mod lock_commands;

pub(crate) use crate::javascript::PackageManager as PackageManagerArg;
pub(crate) use commands::Commands;
pub(crate) use commands::LogsArgs;
pub(crate) use commands::OpenArgs;
pub(crate) use commands::PhpToolArgs;
pub(crate) use commands::SetupArgs;
pub(crate) use commands::{
    BenchmarkEvidenceScenario, DaemonAdoptArgs, DaemonArgs, DaemonBackupArgs, DaemonBackupsArgs,
    DaemonBenchmarkArgs, DaemonCommands, DaemonMigrationArgs, DaemonMigrationCommands,
    DaemonMigrationDecisionArgs, DaemonMigrationStatusArgs, DaemonPruneArgs, DaemonPruneCommands,
    DaemonPruneExecuteArgs, DaemonPrunePlanArgs, DaemonRestoreArgs, DaemonRetainedArgs,
    DaemonServiceArgs, DaemonServiceCommands, DaemonServiceInstallArgs, DaemonServicePrintArgs,
    DaemonServiceUninstallArgs, DaemonTrustArgs, DaemonTrustCommands, DaemonWatchArgs,
};
pub(crate) use config_commands::ConfigCommands;
pub(crate) use env_commands::EnvCommands;
pub(crate) use lock_commands::LockCommands;

#[derive(Parser)]
#[command(name = "stackctl", about = "Local development control plane", version)]
#[non_exhaustive]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
    #[arg(global = true, long, short)]
    pub(crate) quiet: bool,
    #[arg(global = true, long)]
    pub(crate) no_color: bool,
    #[arg(global = true, long)]
    pub(crate) dry_run: bool,
    #[arg(
        global = true,
        long,
        value_name = "PATH",
        conflicts_with = "project_root"
    )]
    pub(crate) config: Option<PathBuf>,
    #[arg(global = true, long, value_name = "DIR")]
    pub(crate) project_root: Option<PathBuf>,
    /// Disable interactive prompts and TTY-dependent behavior
    #[arg(global = true, long, default_value_t = false)]
    pub(crate) non_interactive: bool,
}

impl Cli {
    pub(crate) fn config_path(&self) -> Option<&Path> {
        self.config.as_deref()
    }

    pub(crate) fn project_root_path(&self) -> Option<&Path> {
        self.project_root.as_deref()
    }
}

#[cfg(test)]
mod tests;
