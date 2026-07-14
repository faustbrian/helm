//! cli args commands meta module.
//!
//! Contains cli args commands meta logic used by Stackctl command workflows.

use clap::Args;

use crate::cli::args::{ConfigCommands, LockCommands};

#[derive(Args)]
pub(crate) struct ConfigArgs {
    #[command(subcommand)]
    pub(crate) command: ConfigCommands,
}

#[derive(Args)]
pub(crate) struct LockArgs {
    #[command(subcommand)]
    pub(crate) command: LockCommands,
}

#[derive(Args)]
pub(crate) struct CompletionsArgs {
    #[arg()]
    pub(crate) shell: clap_complete::Shell,
}
