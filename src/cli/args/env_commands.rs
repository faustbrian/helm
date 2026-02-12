use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub(crate) enum EnvCommands {
    /// Generate a full env file from managed Helm app variables
    Generate {
        #[arg(long)]
        output: PathBuf,
    },
}
