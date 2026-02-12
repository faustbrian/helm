use anyhow::Result;

use crate::cli::args::{Cli, Commands};
use crate::cli::handlers;
use crate::config::Config;

pub(super) fn dispatch(cli: &Cli, config: &mut Config) -> Option<Result<()>> {
    match &cli.command {
        Commands::Connect(args) => Some(handlers::handle_connect(
            config,
            args.service.as_deref(),
            args.tty,
            args.no_tty,
        )),
        Commands::Url(args) => Some(handlers::handle_url(
            config,
            args.service.as_deref(),
            &args.format,
            args.kind,
            args.driver,
        )),
        _ => None,
    }
}
