use anyhow::Result;

use crate::cli::args::{Cli, Commands};
use crate::cli::handlers;
use crate::config;

pub(super) fn dispatch(cli: &Cli, config: &mut config::Config) -> Option<Result<()>> {
    match &cli.command {
        Commands::List(args) => Some(handlers::handle_list(
            config,
            &args.format,
            args.kind,
            args.driver,
        )),
        Commands::Swarm(args) => Some(handlers::handle_swarm(
            config,
            &args.command,
            &args.only,
            args.without_deps,
            args.force_down_deps,
            args.parallel,
            args.fail_fast,
            args.force_random_ports,
            args.port_strategy,
            args.port_seed.as_deref(),
            args.write_env,
            cli.quiet,
            cli.no_color,
            cli.dry_run,
            cli.repro,
            cli.env.as_deref(),
            cli.config.as_deref(),
            cli.project_root.as_deref(),
        )),
        Commands::Serve(args) => Some(handlers::handle_serve(
            config,
            args.target.as_deref(),
            args.recreate,
            args.detached,
            args.write_env,
            args.trust_container_ca,
            cli.env.as_deref(),
            cli.config.as_deref(),
            cli.project_root.as_deref(),
            &cli.config,
            &cli.project_root,
        )),
        Commands::Open(args) => Some(handlers::handle_open(
            config,
            args.target.as_deref(),
            args.all,
            args.health_path.as_deref(),
            args.no_browser,
            args.json,
        )),
        Commands::EnvScrub(args) => Some(handlers::handle_env_scrub(
            &cli.config,
            &cli.project_root,
            &args.env_file,
            cli.env.as_deref(),
        )),
        _ => None,
    }
}
