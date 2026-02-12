use clap::Args;

use crate::cli::args::{ConfigCommands, LockCommands, PresetCommands, ProfileCommands};

#[derive(Args)]
pub(crate) struct ConfigArgs {
    #[command(subcommand)]
    pub(crate) command: Option<ConfigCommands>,
    #[arg(long, default_value = "toml")]
    pub(crate) format: String,
}

#[derive(Args)]
pub(crate) struct PresetArgs {
    #[command(subcommand)]
    pub(crate) command: PresetCommands,
}

#[derive(Args)]
pub(crate) struct ProfileArgs {
    #[command(subcommand)]
    pub(crate) command: ProfileCommands,
}

#[derive(Args)]
pub(crate) struct LockArgs {
    #[command(subcommand)]
    pub(crate) command: LockCommands,
}

#[derive(Args)]
pub(crate) struct DoctorArgs {
    /// Attempt to fix detected issues where possible
    #[arg(long, default_value_t = false)]
    pub(crate) fix: bool,
    /// Run reproducibility checks (lockfile + immutable image references)
    #[arg(long, default_value_t = false)]
    pub(crate) repro: bool,
}

#[derive(Args)]
pub(crate) struct CompletionsArgs {
    #[arg()]
    pub(crate) shell: clap_complete::Shell,
}
