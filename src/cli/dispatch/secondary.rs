use anyhow::Result;

use crate::cli::args::{Cli, Commands};
use crate::config;

mod data;
mod exec;
mod serve;

pub(super) fn dispatch_secondary(cli: &Cli, config: &mut config::Config) -> Result<()> {
    if let Some(result) = data::dispatch(cli, config) {
        return result;
    }

    if let Some(result) = exec::dispatch(cli, config) {
        return result;
    }

    if let Some(result) = serve::dispatch(cli, config) {
        return result;
    }

    match &cli.command {
        Commands::Init
        | Commands::Completions(_)
        | Commands::Config(_)
        | Commands::Preset(_)
        | Commands::Profile(_)
        | Commands::Doctor(_)
        | Commands::Lock(_)
        | Commands::Setup(_)
        | Commands::Up(_)
        | Commands::Apply(_)
        | Commands::Update(_)
        | Commands::Down(_)
        | Commands::Stop(_)
        | Commands::Rm(_)
        | Commands::Recreate(_)
        | Commands::Restart(_)
        | Commands::Connect(_)
        | Commands::Url(_) => Ok(()),
        _ => Ok(()),
    }
}
