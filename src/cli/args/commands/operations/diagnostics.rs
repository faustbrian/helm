//! Strict v8 status and managed-environment arguments.

use clap::Args;

use crate::cli::args::EnvCommands;

#[derive(Args)]
pub(crate) struct PsArgs {
    #[arg(long, default_value = "table")]
    pub(crate) format: String,
}

#[derive(Args)]
pub(crate) struct EnvArgs {
    #[command(subcommand)]
    pub(crate) command: EnvCommands,
}
