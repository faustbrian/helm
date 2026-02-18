//! Dispatch for container management commands (`stop`, `rm`).

use anyhow::Result;

use crate::cli::args::{Cli, Commands};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::cli::handlers;
use crate::config::Config;

/// Dispatches management commands, returning `None` for non-management commands.
pub(super) fn dispatch(
    cli: &Cli,
    config: &mut Config,
    context: &CliDispatchContext<'_>,
) -> Option<Result<()>> {
    match &cli.command {
        Commands::Stop(args) => Some(handlers::handle_stop(
            config,
            args.service(),
            args.kind(),
            args.parallel,
            context.quiet(),
        )),
        Commands::Rm(args) => Some(handlers::handle_rm(
            config,
            handlers::HandleRmOptions {
                service: args.service(),
                kind: args.kind(),
                force: args.force,
                parallel: args.parallel,
                quiet: context.quiet(),
            },
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::cli::{args::Cli, dispatch::context::CliDispatchContext};
    use crate::config::Config;
    use clap::Parser;

    fn sample_config() -> Config {
        Config {
            schema_version: 1,
            container_prefix: None,
            service: Vec::new(),
            swarm: Vec::new(),
        }
    }

    fn dispatch_result(args: &[&str]) -> Option<Result<(), anyhow::Error>> {
        let cli = Cli::parse_from(args);
        let context = CliDispatchContext::from_cli(&cli);
        let mut config = sample_config();
        super::dispatch(&cli, &mut config, &context)
    }

    #[test]
    fn management_dispatches_stop() {
        assert!(dispatch_result(&["helm", "stop"]).is_some());
    }

    #[test]
    fn management_dispatches_rm() {
        assert!(dispatch_result(&["helm", "rm"]).is_some());
    }

    #[test]
    fn management_ignores_other_commands() {
        assert!(dispatch_result(&["helm", "up"]).is_none());
    }
}
