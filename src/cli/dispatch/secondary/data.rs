//! Dispatch for data/diagnostics commands.
//!
//! Includes commands like `about`, `restore`, `dump`, `health`, `env`, `logs`,
//! and `pull`.

use anyhow::Result;

use crate::cli::args::{Cli, Commands};
use crate::cli::handlers;
use crate::config;

/// Attempts to dispatch a command in the data/diagnostics group.
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
        Commands::Ps(args) => Some(handlers::handle_status(
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
        Commands::Top(args) => Some(handlers::handle_top(
            config,
            args.service.as_deref(),
            args.kind,
            &args.args,
        )),
        Commands::Stats(args) => Some(handlers::handle_stats(
            config,
            args.service.as_deref(),
            args.kind,
            args.no_stream,
            args.format.as_deref(),
        )),
        Commands::Inspect(args) => Some(handlers::handle_inspect(
            config,
            args.service.as_deref(),
            args.kind,
            args.format.as_deref(),
            args.json,
            args.size,
            args.object_type.as_deref(),
        )),
        Commands::Attach(args) => Some(handlers::handle_attach(
            config,
            args.service.as_deref(),
            args.no_stdin,
            args.sig_proxy,
            args.detach_keys.as_deref(),
        )),
        Commands::Cp(args) => Some(handlers::handle_cp(
            config,
            &args.source,
            &args.destination,
            args.follow_link,
            args.archive,
        )),
        Commands::Kill(args) => Some(handlers::handle_kill(
            config,
            args.service.as_deref(),
            args.kind,
            args.signal.as_deref(),
            args.parallel,
        )),
        Commands::Pause(args) => Some(handlers::handle_pause(
            config,
            args.service.as_deref(),
            args.kind,
            args.parallel,
        )),
        Commands::Unpause(args) => Some(handlers::handle_unpause(
            config,
            args.service.as_deref(),
            args.kind,
            args.parallel,
        )),
        Commands::Wait(args) => Some(handlers::handle_wait(
            config,
            args.service.as_deref(),
            args.kind,
            args.condition.as_deref(),
            args.parallel,
        )),
        Commands::Events(args) => Some(handlers::handle_events(
            config,
            args.service.as_deref(),
            args.kind,
            args.since.as_deref(),
            args.until.as_deref(),
            args.format.as_deref(),
            args.json,
            args.all,
            args.allow_empty,
            &args.filter,
        )),
        Commands::Port(args) => Some(handlers::handle_port(
            config,
            args.service.as_deref(),
            args.kind,
            args.json,
            args.private_port.as_deref(),
        )),
        Commands::Prune(args) => Some(handlers::handle_prune(
            config,
            args.service.as_deref(),
            args.kind,
            args.parallel,
            args.all,
            args.force,
            &args.filter,
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
