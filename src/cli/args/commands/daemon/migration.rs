use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Args)]
pub(crate) struct DaemonMigrationArgs {
    #[command(subcommand)]
    pub(crate) command: DaemonMigrationCommands,
}

#[derive(Subcommand)]
pub(crate) enum DaemonMigrationCommands {
    /// Inventory one legacy project without mutating its source
    Inventory(DaemonMigrationInventoryArgs),
    /// Show durable migration checkpoints for one exact project
    Status(DaemonMigrationStatusArgs),
    /// Permanently accept one reversible migration and retire its source
    Confirm(DaemonMigrationDecisionArgs),
    /// Return one reversible migration to its retained source
    Rollback(DaemonMigrationDecisionArgs),
}

/// Selects one legacy project below an authoritative watched root.
#[derive(Args)]
pub(crate) struct DaemonMigrationInventoryArgs {
    /// Existing legacy project directory containing .stackctl.toml
    #[arg(default_value = ".", value_name = "PATH")]
    pub(crate) path: PathBuf,
}

/// Selects the exact registered project whose migrations should be inspected.
#[derive(Args)]
pub(crate) struct DaemonMigrationStatusArgs {
    /// Existing project directory registered by the singleton daemon
    #[arg(default_value = ".", value_name = "PATH")]
    pub(crate) path: PathBuf,
}

/// Selects one exact reversible migration and its registered project.
#[derive(Args)]
pub(crate) struct DaemonMigrationDecisionArgs {
    /// Durable migration identifier reported by restore or migration status
    #[arg(value_name = "MIGRATION_ID")]
    pub(crate) migration_id: String,
    /// Existing project directory registered by the singleton daemon
    #[arg(default_value = ".", value_name = "PATH")]
    pub(crate) path: PathBuf,
}
