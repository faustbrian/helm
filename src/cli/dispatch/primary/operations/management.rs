use anyhow::Result;

use crate::cli::args::{Cli, Commands};
use crate::cli::handlers;
use crate::config::Config;

pub(super) fn dispatch(cli: &Cli, config: &mut Config) -> Option<Result<()>> {
    match &cli.command {
        Commands::Stop(args) => Some(handlers::handle_stop(
            config,
            args.service.as_deref(),
            args.kind,
            args.parallel,
            cli.quiet,
        )),
        Commands::Rm(args) => Some(handlers::handle_rm(
            config,
            args.service.as_deref(),
            args.kind,
            args.force,
            args.parallel,
            cli.quiet,
        )),
        _ => None,
    }
}
