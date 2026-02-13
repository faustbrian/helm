//! Lifecycle command dispatch (`up`, `apply`, `update`, `down`, `recreate`, `restart`).

use anyhow::Result;

use crate::cli::args::{Cli, Commands, PullPolicyArg};
use crate::cli::handlers;
use crate::config::Config;
use crate::docker;

mod up;

/// Dispatches lifecycle commands and forwards normalized arguments to handlers.
///
/// `apply` is mapped to a deterministic `up` invocation with fixed defaults.
pub(super) fn dispatch(cli: &Cli, config: &mut Config) -> Option<Result<()>> {
    match &cli.command {
        Commands::Up(args) => Some(up::dispatch_up(
            cli,
            config,
            args.service.as_deref(),
            args.kind,
            args.profile.as_deref(),
            args.wait,
            args.no_wait,
            args.wait_timeout,
            args.pull,
            args.force_recreate,
            args.publish_all,
            args.no_publish_all,
            args.port_strategy,
            args.port_seed.as_deref(),
            args.save_ports,
            args.env_output,
            !args.no_deps,
            args.seed,
            args.parallel,
        )),
        Commands::Start(args) => Some(handlers::handle_start(
            config,
            args.service.as_deref(),
            args.kind,
            args.profile.as_deref(),
            args.wait,
            args.no_wait,
            args.wait_timeout,
            pull_policy_from_arg(args.pull),
            args.force_recreate,
            !args.no_open,
            args.health_path.as_deref(),
            !args.no_deps,
            args.parallel,
            cli.quiet,
            cli.no_color,
            cli.dry_run,
            cli.repro,
            cli.env.as_deref(),
            cli.config.as_deref(),
            cli.project_root.as_deref(),
            &cli.config,
            &cli.project_root,
        )),
        Commands::Apply(args) => Some(up::dispatch_up(
            cli,
            config,
            None,
            None,
            None,
            true,
            false,
            30,
            PullPolicyArg::Missing,
            false,
            false,
            false,
            crate::cli::args::PortStrategyArg::Random,
            None,
            false,
            false,
            !args.no_deps,
            true,
            1,
        )),
        Commands::Update(args) => Some(handlers::handle_update(
            config,
            args.service.as_deref(),
            args.kind,
            args.profile.as_deref(),
            args.force_recreate,
            args.no_build,
            args.wait,
            args.wait_timeout,
            cli.config.as_deref(),
            cli.project_root.as_deref(),
        )),
        Commands::Down(args) => Some(handlers::handle_down(
            config,
            args.service.as_deref(),
            args.kind,
            !args.no_deps,
            args.force,
            args.parallel,
            cli.quiet,
            cli.no_color,
            cli.dry_run,
            cli.env.as_deref(),
            cli.config.as_deref(),
            cli.project_root.as_deref(),
        )),
        Commands::Recreate(args) => Some(handlers::handle_recreate(
            config,
            args.service.as_deref(),
            args.kind,
            args.wait,
            args.wait_timeout,
            args.publish_all,
            args.save_ports,
            args.env_output,
            args.parallel,
            cli.quiet,
            cli.env.as_deref(),
            cli.config.as_deref(),
            cli.project_root.as_deref(),
            &cli.config,
            &cli.project_root,
        )),
        Commands::Restart(args) => Some(handlers::handle_restart(
            config,
            args.service.as_deref(),
            args.kind,
            args.wait,
            args.wait_timeout,
            args.parallel,
        )),
        Commands::Relabel(args) => Some(handlers::handle_relabel(
            config,
            args.service.as_deref(),
            args.kind,
            args.wait,
            args.wait_timeout,
            args.parallel,
            cli.config.as_deref(),
            cli.project_root.as_deref(),
        )),
        _ => None,
    }
}

fn pull_policy_from_arg(arg: PullPolicyArg) -> docker::PullPolicy {
    match arg {
        PullPolicyArg::Always => docker::PullPolicy::Always,
        PullPolicyArg::Missing => docker::PullPolicy::Missing,
        PullPolicyArg::Never => docker::PullPolicy::Never,
    }
}
