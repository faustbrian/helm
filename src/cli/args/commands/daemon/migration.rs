use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Args)]
pub(crate) struct DaemonMigrationArgs {
    #[command(subcommand)]
    pub(crate) command: DaemonMigrationCommands,
}

#[derive(Subcommand)]
pub(crate) enum DaemonMigrationCommands {
    /// Show durable migration checkpoints for one exact project
    Status(DaemonMigrationStatusArgs),
}

/// Selects the exact registered project whose migrations should be inspected.
#[derive(Args)]
pub(crate) struct DaemonMigrationStatusArgs {
    /// Existing project directory registered by the singleton daemon
    #[arg(default_value = ".", value_name = "PATH")]
    pub(crate) path: PathBuf,
}
