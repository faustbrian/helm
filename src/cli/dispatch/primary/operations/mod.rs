//! Dispatch for operational commands (lifecycle, management, access, setup).

use anyhow::Result;

use crate::cli::args::{Cli, Commands};
use crate::config::Config;

mod access;
mod lifecycle;
mod management;

/// Dispatches operational commands in precedence order.
///
/// `lifecycle`, then `management`, then `access`, then fallback `setup`.
pub(super) fn dispatch_operation_commands(cli: &Cli, config: &mut Config) -> Option<Result<()>> {
    lifecycle::dispatch(cli, config)
        .or_else(|| management::dispatch(cli, config))
        .or_else(|| access::dispatch(cli, config))
        .or_else(|| match &cli.command {
            Commands::Setup(args) => Some(crate::cli::support::for_each_service(
                config,
                args.service.as_deref(),
                args.kind,
                None,
                args.parallel,
                |svc| crate::database::setup(svc, args.timeout),
            )),
            _ => None,
        })
}
