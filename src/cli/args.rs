use clap::Parser;
use std::path::PathBuf;

mod arg_enums;
mod commands;
mod config_commands;
mod env_commands;
mod lock_commands;
mod preset_commands;
mod profile_commands;

pub(crate) use arg_enums::{PackageManagerArg, PortStrategyArg, PullPolicyArg};
pub(crate) use commands::Commands;
pub(crate) use config_commands::ConfigCommands;
pub(crate) use env_commands::EnvCommands;
pub(crate) use lock_commands::LockCommands;
pub(crate) use preset_commands::PresetCommands;
pub(crate) use profile_commands::ProfileCommands;

#[derive(Parser)]
#[command(name = "helm", about = "Local data service manager", version)]
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
    /// Runtime environment namespace (for example: `test`)
    #[arg(global = true, long, value_name = "NAME")]
    pub(crate) env: Option<String>,
    /// Enable reproducible mode (deterministic behavior with lockfile checks)
    #[arg(global = true, long, default_value_t = false)]
    pub(crate) repro: bool,
}
