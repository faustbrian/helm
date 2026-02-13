//! Argument translation for `up`-style lifecycle commands.
//!
//! This module converts CLI enums/flags into the handler-level types consumed by
//! `handle_up`.

use anyhow::Result;

use crate::cli::args::PortStrategyArg;
use crate::cli::args::PullPolicyArg;
use crate::cli::handlers;
use crate::config::{Config, Kind};
use crate::docker;

/// Dispatches `up`/`apply` after translating CLI-level arguments.
#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_up(
    cli: &crate::cli::args::Cli,
    config: &mut Config,
    service: Option<&str>,
    kind: Option<Kind>,
    profile: Option<&str>,
    wait: bool,
    no_wait: bool,
    wait_timeout: u64,
    pull: PullPolicyArg,
    force_recreate: bool,
    publish_all: bool,
    no_publish_all: bool,
    port_strategy: PortStrategyArg,
    port_seed: Option<&str>,
    save_ports: bool,
    env_output: bool,
    include_project_deps: bool,
    seed: bool,
    parallel: usize,
) -> Result<()> {
    handlers::handle_up(
        config,
        service,
        kind,
        profile,
        wait,
        no_wait,
        wait_timeout,
        pull_policy_from_arg(pull),
        force_recreate,
        publish_all,
        no_publish_all,
        port_strategy,
        port_seed,
        save_ports,
        env_output,
        include_project_deps,
        seed,
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

/// Converts the CLI pull policy enum to the docker execution enum.
fn pull_policy_from_arg(arg: PullPolicyArg) -> docker::PullPolicy {
    match arg {
        PullPolicyArg::Always => docker::PullPolicy::Always,
        PullPolicyArg::Missing => docker::PullPolicy::Missing,
        PullPolicyArg::Never => docker::PullPolicy::Never,
    }
}
