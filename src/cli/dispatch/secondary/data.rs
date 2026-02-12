use anyhow::Result;

use crate::cli::args::{Cli, Commands};
use crate::cli::handlers;
use crate::config;

pub(super) fn dispatch(cli: &Cli, config: &mut config::Config) -> Option<Result<()>> {
    match &cli.command {
        Commands::About(_) => Some(handlers::handle_about(
            config,
            cli.env.as_deref(),
            cli.config.as_deref(),
            cli.project_root.as_deref(),
        )),
        Commands::Restore(args) => Some(handlers::handle_restore(
            config,
            args.service.as_deref(),
            args.file.as_ref(),
            args.reset,
            args.migrate,
            args.schema_dump,
            args.gzip,
            cli.project_root.as_deref(),
            cli.config.as_deref(),
        )),
        Commands::Dump(args) => Some(handlers::handle_dump(
            config,
            args.service.as_deref(),
            args.file.as_ref(),
            args.stdout,
            args.gzip,
        )),
        Commands::Status(args) => Some(handlers::handle_status(
            config,
            &args.format,
            args.kind,
            args.driver,
        )),
        Commands::Health(args) => Some(handlers::handle_health(
            config,
            args.service.as_deref(),
            args.kind,
            args.timeout,
            args.interval,
            args.retries,
            args.parallel,
        )),
        Commands::Env(args) => Some(handlers::handle_env(
            config,
            args.command.as_ref(),
            args.service.as_deref(),
            args.kind,
            &args.env_file,
            args.sync,
            args.purge,
            args.persist_runtime,
            args.create_missing,
            cli.quiet,
            &cli.config,
            &cli.project_root,
            cli.env.as_deref(),
        )),
        Commands::Logs(args) => Some(handlers::handle_logs(
            config,
            args.service.as_deref(),
            args.kind,
            args.all,
            args.follow,
            args.tail,
            args.timestamps,
            args.prefix,
            args.access,
        )),
        Commands::Pull(args) => Some(handlers::handle_pull(
            config,
            args.service.as_deref(),
            args.kind,
            args.parallel,
        )),
        _ => None,
    }
}
