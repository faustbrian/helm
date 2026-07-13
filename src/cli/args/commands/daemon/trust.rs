//! CLI arguments for singleton Stackctl CA trust workflows.

use clap::{Args, Subcommand};

#[derive(Args)]
pub(crate) struct DaemonTrustArgs {
    #[command(subcommand)]
    pub(crate) command: DaemonTrustCommands,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Subcommand)]
pub(crate) enum DaemonTrustCommands {
    /// Create or recover the singleton CA and add it to platform trust
    Install,
    /// Inspect trust for the exact current singleton CA
    Status,
    /// Remove trust for the exact current singleton CA
    Remove,
}
