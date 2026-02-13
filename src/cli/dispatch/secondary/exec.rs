//! Dispatch for command-execution style commands.
//!
//! Covers `exec`, `artisan`, `composer`, `node`, and `app-create`.

use anyhow::Result;

use crate::cli::args::PackageManagerArg;
use crate::cli::args::{Cli, Commands};
use crate::cli::handlers;
use crate::config;

/// Attempts to dispatch a command in the execution group.
pub(super) fn dispatch(cli: &Cli, config: &mut config::Config) -> Option<Result<()>> {
    match &cli.command {
        Commands::Exec(args) => Some(handlers::handle_exec(
            config,
            args.service.as_deref(),
            args.tty,
            args.no_tty,
            &args.command,
        )),
        Commands::Artisan(args) => Some(handlers::handle_artisan(
            config,
            args.service.as_deref(),
            args.tty,
            args.no_tty,
            &args.command,
            cli.config.as_deref(),
            cli.project_root.as_deref(),
        )),
        Commands::Composer(args) => Some(handlers::handle_composer(
            config,
            args.service.as_deref(),
            args.tty,
            args.no_tty,
            &args.command,
            cli.config.as_deref(),
            cli.project_root.as_deref(),
        )),
        Commands::Node(args) => {
            let manager_bin = match args.manager {
                PackageManagerArg::Bun => "bun",
                PackageManagerArg::Npm => "npm",
                PackageManagerArg::Pnpm => "pnpm",
                PackageManagerArg::Yarn => "yarn",
            };
            Some(handlers::handle_node(
                config,
                args.service.as_deref(),
                manager_bin,
                args.tty,
                args.no_tty,
                &args.command,
                cli.config.as_deref(),
                cli.project_root.as_deref(),
            ))
        }
        Commands::AppCreate(args) => Some(handlers::handle_app_create(
            config,
            args.service.as_deref(),
            args.no_migrate,
            args.seed,
            args.no_storage_link,
            cli.config.as_deref(),
            cli.project_root.as_deref(),
        )),
        _ => None,
    }
}
