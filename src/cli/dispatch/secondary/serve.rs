//! Dispatch for serve/swarm-facing commands.
//!
//! Covers `ls`, `swarm`, `serve`, `open`, and `env-scrub`.

use anyhow::Result;

use crate::cli::args::Cli;
use crate::cli::dispatch::context::CliDispatchContext;
use crate::config;

mod listing;
mod orchestration;

/// Attempts to dispatch a command in the serve/swarm group.
pub(super) fn dispatch(
    cli: &Cli,
    config: &mut config::Config,
    context: &CliDispatchContext<'_>,
) -> Option<Result<()>> {
    listing::dispatch(cli, config, context)
        .or_else(|| orchestration::dispatch(cli, config, context))
}
