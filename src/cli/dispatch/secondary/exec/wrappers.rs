//! Secondary exec dispatch for wrapper-style commands.

use anyhow::Result;

use crate::cli::args::PackageManagerArg;
use crate::cli::args::{Cli, Commands};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::cli::handlers;
use crate::config;

pub(super) fn dispatch(
    cli: &Cli,
    config: &mut config::Config,
    context: &CliDispatchContext<'_>,
) -> Option<Result<()>> {
    match &cli.command {
        Commands::Exec(args) => Some(handlers::handle_exec(
            config,
            handlers::HandleExecOptions {
                service: args.service(),
                kind: args.kind,
                profile: args.profile(),
                non_interactive: context.non_interactive(),
                tty: args.tty,
                no_tty: args.no_tty,
                command: &args.command,
            },
        )),
        Commands::Artisan(args) => Some(handlers::handle_artisan(
            config,
            handlers::HandleArtisanOptions {
                service: args.service(),
                kind: args.kind,
                profile: args.profile(),
                non_interactive: context.non_interactive(),
                tty: args.tty,
                no_tty: args.no_tty,
                command: &args.command,
                config_path: context.config_path(),
                project_root: context.project_root(),
            },
        )),
        Commands::Composer(args) => Some(handlers::handle_package_manager_command(
            config,
            handlers::HandlePackageManagerCommandOptions {
                service: args.service(),
                kind: args.kind,
                profile: args.profile(),
                manager_bin: "composer",
                non_interactive: context.non_interactive(),
                tty: args.tty,
                no_tty: args.no_tty,
                command: &args.command,
                config_path: context.config_path(),
                project_root: context.project_root(),
                usage_error: "No composer command specified. Usage: helm composer [--service <name>] -- <command>",
            },
        )),
        Commands::Node(args) => Some(handlers::handle_package_manager_command(
            config,
            handlers::HandlePackageManagerCommandOptions {
                service: args.service(),
                kind: args.kind,
                profile: args.profile(),
                manager_bin: manager_bin(args.manager),
                non_interactive: context.non_interactive(),
                tty: args.tty,
                no_tty: args.no_tty,
                command: &args.command,
                config_path: context.config_path(),
                project_root: context.project_root(),
                usage_error: "No package manager command specified. Usage: helm node [--manager bun|npm|pnpm|yarn] [--service <name>] -- <command>",
            },
        )),
        Commands::AppCreate(args) => Some(handlers::handle_app_create(
            config,
            handlers::HandleAppCreateOptions {
                service: args.service(),
                no_migrate: args.no_migrate,
                seed: args.seed,
                no_storage_link: args.no_storage_link,
                config_path: context.config_path(),
                project_root: context.project_root(),
            },
        )),
        _ => None,
    }
}

const fn manager_bin(manager: PackageManagerArg) -> &'static str {
    match manager {
        PackageManagerArg::Bun => "bun",
        PackageManagerArg::Npm => "npm",
        PackageManagerArg::Pnpm => "pnpm",
        PackageManagerArg::Yarn => "yarn",
    }
}

#[cfg(test)]
mod tests {
    use super::manager_bin;
    use crate::cli::{
        args::{Cli, PackageManagerArg},
        dispatch::context::CliDispatchContext,
    };
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
    fn manager_bin_maps_package_managers() {
        assert_eq!(manager_bin(PackageManagerArg::Bun), "bun");
        assert_eq!(manager_bin(PackageManagerArg::Npm), "npm");
        assert_eq!(manager_bin(PackageManagerArg::Pnpm), "pnpm");
        assert_eq!(manager_bin(PackageManagerArg::Yarn), "yarn");
    }

    #[test]
    fn wrapper_dispatches_exec() {
        assert!(dispatch_result(&["helm", "exec"]).is_some());
    }

    #[test]
    fn wrapper_dispatches_artisan() {
        assert!(dispatch_result(&["helm", "artisan"]).is_some());
    }

    #[test]
    fn wrapper_dispatches_composer() {
        assert!(dispatch_result(&["helm", "composer"]).is_some());
    }

    #[test]
    fn wrapper_dispatches_node() {
        assert!(dispatch_result(&["helm", "node"]).is_some());
    }

    #[test]
    fn wrapper_dispatches_app_create() {
        assert!(dispatch_result(&["helm", "app-create"]).is_some());
    }

    #[test]
    fn wrapper_dispatches_none_for_other_commands() {
        assert!(dispatch_result(&["helm", "status"]).is_none());
    }
}
