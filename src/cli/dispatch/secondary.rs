//! Secondary command dispatch.
//!
//! This runs after primary dispatch and covers data commands, command execution
//! wrappers, and serve/swarm-oriented commands.

use anyhow::Result;

use crate::cli::args::{Cli, Commands};
use crate::config;

mod data;
mod exec;
mod serve;

/// Dispatches commands handled by secondary subtrees.
///
/// This function intentionally returns `Ok(())` for commands already consumed by
/// earlier dispatch layers.
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
        | Commands::Start(_)
        | Commands::Up(_)
        | Commands::Apply(_)
        | Commands::Update(_)
        | Commands::Down(_)
        | Commands::Stop(_)
        | Commands::Rm(_)
        | Commands::Recreate(_)
        | Commands::Restart(_)
        | Commands::Ls(_)
        | Commands::Ps(_)
        | Commands::Url(_) => Ok(()),
        _ => Ok(()),
    }
}
