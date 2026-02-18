//! Secondary serve dispatch for listing/open commands.

use anyhow::Result;

use crate::cli::args::{Cli, Commands};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::cli::handlers;
use crate::config;

pub(super) fn dispatch(
    cli: &Cli,
    config: &mut config::Config,
    _context: &CliDispatchContext<'_>,
) -> Option<Result<()>> {
    match &cli.command {
        Commands::Ls(args) => Some(handlers::handle_list(
            config,
            &args.format,
            args.kind(),
            args.driver(),
        )),
        Commands::Open(args) => Some(handlers::handle_open(
            config,
            handlers::HandleOpenOptions {
                service: args.service(),
                all: args.all,
                health_path: args.health_path(),
                no_browser: args.no_browser,
                json: args.json,
            },
        )),
        _ => None,
    }
}
