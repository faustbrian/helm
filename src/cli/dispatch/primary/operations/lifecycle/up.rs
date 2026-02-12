use anyhow::Result;

use crate::cli::args::PortStrategyArg;
use crate::cli::args::PullPolicyArg;
use crate::cli::handlers;
use crate::config::{Config, Kind};
use crate::docker;

#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_up(
    cli: &crate::cli::args::Cli,
    config: &mut Config,
    service: Option<&str>,
    kind: Option<Kind>,
    profile: Option<&str>,
    healthy: bool,
    timeout: u64,
    pull: PullPolicyArg,
    recreate: bool,
    random_ports: bool,
    force_random_ports: bool,
    port_strategy: PortStrategyArg,
    port_seed: Option<&str>,
    persist_ports: bool,
    write_env: bool,
    with_project_deps: bool,
    with_data: bool,
    parallel: usize,
) -> Result<()> {
    handlers::handle_up(
        config,
        service,
        kind,
        profile,
        healthy,
        timeout,
        pull_policy_from_arg(pull),
        recreate,
        random_ports,
        force_random_ports,
        port_strategy,
        port_seed,
        persist_ports,
        write_env,
        with_project_deps,
        with_data,
        parallel,
        cli.quiet,
        cli.no_color,
        cli.dry_run,
        cli.repro,
        cli.env.as_deref(),
        cli.config.as_deref(),
        cli.project_root.as_deref(),
        &cli.config,
        &cli.project_root,
    )
}

fn pull_policy_from_arg(arg: PullPolicyArg) -> docker::PullPolicy {
    match arg {
        PullPolicyArg::Always => docker::PullPolicy::Always,
        PullPolicyArg::Missing => docker::PullPolicy::Missing,
        PullPolicyArg::Never => docker::PullPolicy::Never,
    }
}
